use itertools::Itertools;
use skia_safe::{
    canvas::SaveLayerRec,
    image_filters::blur,
    utils::shadow_utils::{draw_shadow, ShadowFlags},
    BlendMode, Canvas, ClipOp, Color, Contains, Paint, Path, PathBuilder, Point, Point3, Rect,
};
use std::hash::{Hash, Hasher};
use std::{cmp::PartialOrd, collections::HashMap};

use crate::dimensions::Dimensions;

use super::{RenderedWindow, RendererSettings, WindowDrawDetails};

const EPSILON: f32 = 1e-6;

#[derive(Debug, Clone)]
struct CornerFromRect {
    p: Point,
    rect_index: Vec<usize>,
}

#[derive(Debug, Clone)]
struct PointWrapper(Point);

fn compare_coordinate(a: f32, b: f32) -> std::cmp::Ordering {
    if (a - b).abs() < EPSILON {
        std::cmp::Ordering::Equal
    } else {
        a.partial_cmp(&b).unwrap()
    }
}

impl PartialEq for PointWrapper {
    fn eq(&self, other: &Self) -> bool {
        (self.0.x - other.0.x).abs() < EPSILON && (self.0.y - other.0.y).abs() < EPSILON
    }
}

impl Eq for PointWrapper {}

impl Hash for PointWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0.x as i32).hash(state);
        (self.0.y as i32).hash(state);
    }
}

impl PartialOrd for PointWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PointWrapper {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        compare_coordinate(self.0.x, other.0.x)
            .then_with(|| compare_coordinate(self.0.y, other.0.y))
    }
}

impl CornerFromRect {
    fn share_rect(&self, other: &CornerFromRect) -> bool {
        self.rect_index.iter().any(|i| other.rect_index.contains(i))
    }

    fn left_to(&self, other: &CornerFromRect) -> bool {
        self.p.x < other.p.x
    }

    fn up_to(&self, other: &CornerFromRect) -> bool {
        self.p.y < other.p.y
    }

    fn same_x(&self, other: &CornerFromRect) -> bool {
        (self.p.x - other.p.x).abs() < EPSILON
    }

    fn same_y(&self, other: &CornerFromRect) -> bool {
        (self.p.y - other.p.y).abs() < EPSILON
    }
}

struct LayerWindow<'w> {
    window: &'w mut RenderedWindow,
    group: usize,
}

pub struct FloatingLayer<'w> {
    pub sort_order: u64,
    pub windows: Vec<&'w mut RenderedWindow>,
}

impl<'w> FloatingLayer<'w> {
    pub fn draw(
        &mut self,
        root_canvas: &Canvas,
        settings: &RendererSettings,
        default_background: Color,
        font_dimensions: Dimensions,
    ) -> Vec<WindowDrawDetails> {
        let pixel_regions = self
            .windows
            .iter()
            .map(|window| window.pixel_region(font_dimensions))
            .collect::<Vec<_>>();
        let (silhouette, bound_rect) = build_slihouette(&pixel_regions);
        let has_transparency = default_background.a() != 255
            || self.windows.iter().any(|window| window.has_transparency());

        self._draw_shadow(root_canvas, &silhouette, settings);

        root_canvas.save();
        root_canvas.clip_path(&silhouette, None, Some(false));
        let need_blur = has_transparency || settings.floating_blur;

        if need_blur {
            if let Some(blur) = blur(
                (
                    settings.floating_blur_amount_x,
                    settings.floating_blur_amount_y,
                ),
                None,
                None,
                None,
            ) {
                let paint = Paint::default()
                    .set_anti_alias(false)
                    .set_blend_mode(BlendMode::Src)
                    .to_owned();
                let save_layer_rec = SaveLayerRec::default()
                    .backdrop(&blur)
                    .bounds(&bound_rect)
                    .paint(&paint);
                root_canvas.save_layer(&save_layer_rec);
                root_canvas.restore();
            }
        }

        let paint = Paint::default()
            .set_anti_alias(false)
            .set_color(Color::from_argb(255, 255, 255, default_background.a()))
            .set_blend_mode(BlendMode::SrcOver)
            .to_owned();

        let save_layer_rec = SaveLayerRec::default().bounds(&bound_rect).paint(&paint);

        root_canvas.save_layer(&save_layer_rec);
        root_canvas.clear(default_background.with_a(255));

        let regions = self
            .windows
            .iter()
            .map(|window| window.pixel_region(font_dimensions))
            .collect::<Vec<_>>();

        let blend = self.uniform_background_blend();

        self.windows.iter_mut().for_each(|window| {
            window.update_blend(blend);
        });

        let mut ret = vec![];

        (0..self.windows.len()).for_each(|i| {
            let window = &mut self.windows[i];
            window.draw_background_surface(root_canvas, &regions[i], font_dimensions);
        });
        (0..self.windows.len()).for_each(|i| {
            let window = &mut self.windows[i];
            window.draw_foreground_surface(root_canvas, &regions[i], font_dimensions);

            ret.push(WindowDrawDetails {
                id: window.id,
                region: regions[i],
                floating_order: window.anchor_info.as_ref().map(|v| v.sort_order),
            });
        });

        root_canvas.restore();

        root_canvas.restore();

        ret
    }

    pub fn uniform_background_blend(&self) -> u8 {
        self.windows
            .iter()
            .filter_map(|window| window.get_smallest_blend_value())
            .min()
            .unwrap_or(0)
    }

    fn _draw_shadow(&self, root_canvas: &Canvas, path: &Path, settings: &RendererSettings) {
        root_canvas.save();
        // We clip using the Difference op to make sure that the shadow isn't rendered inside
        // the window itself.
        root_canvas.clip_path(path, Some(ClipOp::Difference), None);
        // The light angle is specified in degrees from the vertical, so we first convert them
        // to radians and then use sin/cos to get the y and z components of the light
        let light_angle_radians = settings.light_angle_degrees.to_radians();
        draw_shadow(
            root_canvas,
            path,
            // Specifies how far from the root canvas the shadow casting rect is. We just use
            // the z component here to set it a constant distance away.
            Point3::new(0., 0., settings.floating_z_height),
            // Because we use the DIRECTIONAL_LIGHT shadow flag, this specifies the angle that
            // the light is coming from.
            Point3::new(0., -light_angle_radians.sin(), light_angle_radians.cos()),
            // This is roughly equal to the apparent radius of the light .
            5.,
            Color::from_argb((0.03 * 255.) as u8, 0, 0, 0),
            Color::from_argb((0.35 * 255.) as u8, 0, 0, 0),
            // Directional Light flag is necessary to make the shadow render consistently
            // across various sizes of floating windows. It effects how the light direction is
            // processed.
            Some(ShadowFlags::DIRECTIONAL_LIGHT),
        );
        root_canvas.restore();
    }
}

fn get_window_group(windows: &mut Vec<LayerWindow>, index: usize) -> usize {
    if windows[index].group != index {
        windows[index].group = get_window_group(windows, windows[index].group);
    }
    windows[index].group
}

fn rect_intersect(a: &Rect, b: &Rect) -> bool {
    Rect::intersects2(a, b)
}

fn group_windows_with_regions(windows: &mut Vec<LayerWindow>, regions: &[Rect]) {
    for i in 0..windows.len() {
        for j in i + 1..windows.len() {
            let group_i = get_window_group(windows, i);
            let group_j = get_window_group(windows, j);
            if group_i != group_j && rect_intersect(&regions[i], &regions[j]) {
                let new_group = group_i.min(group_j);
                if group_i != group_j {
                    windows[group_i].group = new_group;
                    windows[group_j].group = new_group;
                }
            }
        }
    }
}

pub fn group_windows(
    windows: Vec<&mut RenderedWindow>,
    font_dimensions: Dimensions,
) -> Vec<Vec<&mut RenderedWindow>> {
    let mut windows = windows
        .into_iter()
        .enumerate()
        .map(|(index, window)| LayerWindow {
            window,
            group: index,
        })
        .collect::<Vec<_>>();
    let regions = windows
        .iter()
        .map(|window| window.window.pixel_region(font_dimensions))
        .collect::<Vec<_>>();
    group_windows_with_regions(&mut windows, &regions);
    for i in 0..windows.len() {
        let _ = get_window_group(&mut windows, i);
    }
    windows
        .into_iter()
        .group_by(|window| window.group)
        .into_iter()
        .map(|(_, v)| v.map(|w| w.window).collect::<Vec<_>>())
        .collect_vec()
}

fn inclusive_contains(rect: &Rect, point: &Point) -> bool {
    compare_coordinate(rect.left, point.x) == std::cmp::Ordering::Less
        && compare_coordinate(rect.right, point.x) == std::cmp::Ordering::Greater
        && compare_coordinate(rect.top, point.y) == std::cmp::Ordering::Less
        && compare_coordinate(rect.bottom, point.y) == std::cmp::Ordering::Greater
}

fn build_slihouette(regions: &[Rect]) -> (Path, Rect) {
    let mut corners = calculate_silhouette_corners(regions)
        .into_iter()
        .map(|v| (v, false))
        .collect::<Vec<_>>();
    let mut builder = PathBuilder::new();

    let points = sort_points_in_clockwise_order(&mut corners);
    builder.move_to(points[0]);
    for point in points.iter().skip(1) {
        builder.line_to(*point);
    }
    builder.line_to(points[0]);
    builder.close();

    log::debug!("corners: {:?}, points: {:?}", corners, points);

    (builder.snapshot(), builder.compute_bounds())
}

fn calculate_silhouette_corners(regions: &[Rect]) -> Vec<CornerFromRect> {
    let mut merge_points = HashMap::new();
    (0..regions.len())
        .cartesian_product(0..regions.len())
        .flat_map(|(i, j)| rect_collision_points(regions, i, j))
        .for_each(|p| {
            let point = PointWrapper(p.p);
            merge_points
                .entry(point)
                .or_insert_with(Vec::new)
                .extend(p.rect_index);
        });
    (0..regions.len())
        .flat_map(|i| {
            vec![
                CornerFromRect {
                    p: Point::new(regions[i].left, regions[i].top),
                    rect_index: vec![i],
                },
                CornerFromRect {
                    p: Point::new(regions[i].right, regions[i].top),
                    rect_index: vec![i],
                },
                CornerFromRect {
                    p: Point::new(regions[i].right, regions[i].bottom),
                    rect_index: vec![i],
                },
                CornerFromRect {
                    p: Point::new(regions[i].left, regions[i].bottom),
                    rect_index: vec![i],
                },
            ]
        })
        .for_each(|p| {
            let point = PointWrapper(p.p);
            merge_points
                .entry(point)
                .or_insert_with(Vec::new)
                .extend(p.rect_index);
        });

    let mut points = merge_points
        .into_iter()
        .filter(|p| !regions.iter().any(|r| inclusive_contains(r, &p.0 .0)))
        .map(|(k, v)| CornerFromRect {
            p: k.0,
            rect_index: v.into_iter().unique().collect(),
        })
        .collect_vec();

    points.sort_unstable_by(|a, b| {
        compare_coordinate(a.p.x, b.p.x).then_with(|| compare_coordinate(a.p.y, b.p.y))
    });

    points
}

/// Returns the points of intersection between two rectangles
fn rect_collision_points(regions: &[Rect], i: usize, j: usize) -> Vec<CornerFromRect> {
    let mut intersection = Rect::new_empty();
    if intersection.intersect2(regions[i], regions[j]) {
        vec![
            CornerFromRect {
                p: Point::new(intersection.left, intersection.top),
                rect_index: vec![i, j],
            },
            CornerFromRect {
                p: Point::new(intersection.right, intersection.top),
                rect_index: vec![i, j],
            },
            CornerFromRect {
                p: Point::new(intersection.right, intersection.bottom),
                rect_index: vec![i, j],
            },
            CornerFromRect {
                p: Point::new(intersection.left, intersection.bottom),
                rect_index: vec![i, j],
            },
        ]
    } else {
        vec![]
    }
}

fn sort_points_in_clockwise_order(corners: &mut [(CornerFromRect, bool)]) -> Vec<Point> {
    let mut ret = vec![];
    // PERFORMANCE NOTE: this is a O(n^2) algorithm, it can be optimized
    corners[0].1 = true;
    let mut pivot = Some(corners[0].0.clone());
    let mut direction: Option<MoveDirection> = None;
    ret.push(corners[0].0.p);
    while let Some(current) = pivot {
        if let Some((next, next_direction)) = find_nearest_point(&current, corners, direction) {
            ret.push(next.p);
            pivot = Some(next);
            direction = Some(next_direction);
        } else {
            break;
        }
    }
    ret
}

enum MoveDirection {
    Right,
    Up,
    Down,
    Left,
}

fn try_right(
    pivot: &CornerFromRect,
    points: &mut [&mut (CornerFromRect, bool)],
) -> Option<(CornerFromRect, MoveDirection)> {
    points
        .iter_mut()
        .filter(|(p, _)| pivot.left_to(p) && pivot.same_y(p))
        .min_by(|a, b| compare_coordinate(a.0.p.x, b.0.p.x))
        .map(|(p, used)| {
            *used = true;
            (p.clone(), MoveDirection::Right)
        })
}

fn try_up(
    pivot: &CornerFromRect,
    points: &mut [&mut (CornerFromRect, bool)],
) -> Option<(CornerFromRect, MoveDirection)> {
    points
        .iter_mut()
        .filter(|(p, _)| p.up_to(pivot) && pivot.same_x(p))
        .max_by(|a, b| compare_coordinate(a.0.p.y, b.0.p.y))
        .map(|(p, used)| {
            *used = true;
            (p.clone(), MoveDirection::Up)
        })
}

fn try_down(
    pivot: &CornerFromRect,
    points: &mut [&mut (CornerFromRect, bool)],
) -> Option<(CornerFromRect, MoveDirection)> {
    points
        .iter_mut()
        .filter(|(p, _)| pivot.up_to(p) && pivot.same_x(p))
        .min_by(|a, b| compare_coordinate(a.0.p.y, b.0.p.y))
        .map(|(p, used)| {
            *used = true;
            (p.clone(), MoveDirection::Down)
        })
}

fn try_left(
    pivot: &CornerFromRect,
    points: &mut [&mut (CornerFromRect, bool)],
) -> Option<(CornerFromRect, MoveDirection)> {
    points
        .iter_mut()
        .filter(|(p, _)| p.left_to(pivot) && pivot.same_y(p))
        .max_by(|a, b| compare_coordinate(a.0.p.x, b.0.p.x))
        .map(|(p, used)| {
            *used = true;
            (p.clone(), MoveDirection::Left)
        })
}

// R U D L, clockwise
fn find_nearest_point(
    pivot: &CornerFromRect,
    points: &mut [(CornerFromRect, bool)],
    previous_direction: Option<MoveDirection>,
) -> Option<(CornerFromRect, MoveDirection)> {
    let mut shared_points = points
        .iter_mut()
        .filter(|(p, used)| !used && pivot.share_rect(p))
        .collect::<Vec<_>>();
    let previous_direction = previous_direction.unwrap_or(MoveDirection::Right);
    match previous_direction {
        MoveDirection::Right => {
            if let Some(ret) = try_right(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_up(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_down(pivot, &mut shared_points) {
                Some(ret)
            } else {
                try_left(pivot, &mut shared_points)
            }
        }
        MoveDirection::Up => {
            if let Some(ret) = try_up(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_right(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_left(pivot, &mut shared_points) {
                Some(ret)
            } else {
                try_down(pivot, &mut shared_points)
            }
        }
        MoveDirection::Down => {
            if let Some(ret) = try_down(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_left(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_right(pivot, &mut shared_points) {
                Some(ret)
            } else {
                try_up(pivot, &mut shared_points)
            }
        }
        MoveDirection::Left => {
            if let Some(ret) = try_left(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_down(pivot, &mut shared_points) {
                Some(ret)
            } else if let Some(ret) = try_up(pivot, &mut shared_points) {
                Some(ret)
            } else {
                try_right(pivot, &mut shared_points)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clockwise_paths() {
        let regions: Vec<Rect> = vec![
            Rect::from_xywh(100., 100., 100., 100.),
            Rect::from_xywh(180., 20., 100., 100.),
            Rect::from_xywh(180., 180., 130., 100.),
        ];
        let mut corners = calculate_silhouette_corners(&regions)
            .into_iter()
            .map(|v| (v, false))
            .collect::<Vec<_>>();
        let ret = sort_points_in_clockwise_order(&mut corners);

        println!("{:?}", ret);
        println!("{:?}", corners);
        assert!(ret.len() == corners.len());
        assert_eq!(
            ret,
            vec![
                Point::new(100., 100.),
                Point::new(180., 100.),
                Point::new(180., 20.),
                Point::new(280., 20.),
                Point::new(280., 120.),
                Point::new(200., 120.),
                Point::new(200., 180.),
                Point::new(310., 180.),
                Point::new(310., 280.),
                Point::new(180., 280.),
                Point::new(180., 200.),
                Point::new(100., 200.),
            ]
        );
    }

    #[test]
    fn test_clockwise_paths_telescope_case() {
        let regions = vec![
            Rect::from_ltrb(0., 834., 3420., 912.),
            Rect::from_ltrb(0., 886., 1692., 1328.),
            Rect::from_ltrb(12., 860., 3408., 886.),
            Rect::from_ltrb(12., 886., 1680., 1302.),
            Rect::from_ltrb(1692., 886., 3420., 1328.),
            Rect::from_ltrb(1704., 912., 3408., 1302.),
        ];
        let mut corners = calculate_silhouette_corners(&regions)
            .into_iter()
            .map(|v| (v, false))
            .collect::<Vec<_>>();
        let ret = sort_points_in_clockwise_order(&mut corners);

        println!("{:?}", ret);

        for corner in corners.iter() {
            println!("{:?}", corner);
        }

        assert_eq!(
            ret,
            vec![
                Point::new(0., 834.),
                Point::new(3420., 834.),
                Point::new(3420., 1328.),
                Point::new(0., 1328.),
            ]
        );
    }
}
