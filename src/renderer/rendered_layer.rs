use itertools::Itertools;
use skia_safe::{
    canvas::SaveLayerRec,
    image_filters::blur,
    utils::shadow_utils::{draw_shadow, ShadowFlags},
    BlendMode, Canvas, ClipOp, Color, Paint, Path, PathBuilder, Point, Point3, Rect,
};
use std::hash::{Hash, Hasher};
use std::{cmp::PartialOrd, collections::HashMap};

use crate::dimensions::Dimensions;

use super::{RenderedWindow, RendererSettings, WindowDrawDetails};

const EPSILON: f32 = 1e-6;

#[derive(Debug, Clone, Copy)]
struct PointWrapper(Point);

#[derive(Debug, Clone, Copy)]
struct CoordinateWrapper(f32);

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

impl PartialEq for CoordinateWrapper {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() < EPSILON
    }
}

impl Eq for CoordinateWrapper {}

impl Hash for CoordinateWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as i32).hash(state);
    }
}

impl PartialOrd for CoordinateWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CoordinateWrapper {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        compare_coordinate(self.0, other.0)
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

        self.draw_shadow(root_canvas, &silhouette, settings);

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

    fn draw_shadow(&self, root_canvas: &Canvas, path: &Path, settings: &RendererSettings) {
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

struct SilihouetteCorner {
    pos: Point,
    next: [Option<Point>; 4],
}

fn build_rect_corners(region: &Rect) -> Vec<Point> {
    vec![
        Point::new(region.left, region.top),
        Point::new(region.right, region.top),
        Point::new(region.right, region.bottom),
        Point::new(region.left, region.bottom),
    ]
}

// fn split_rect_edge(region: &Rect, intersection: &Rect) -> Vec<SilihouetteCorner> {
//     let mut ret = vec![];
//     if compare_coordinate(region.left, intersection.left) == std::cmp::Ordering::Equal {
//         if compare_coordinate(intersection.top, intersection.bottom) != std::cmp::Ordering::Equal {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.left, intersection.bottom),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.left, intersection.top),
//                     direction: EdgeDirection::Up,
//                 }],
//             });
//         }
//         if compare_coordinate(region.top, intersection.top) == std::cmp::Ordering::Less {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.left, intersection.top),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(region.left, region.top),
//                     direction: EdgeDirection::Up,
//                 }],
//             });
//         }
//         if compare_coordinate(region.bottom, intersection.bottom) == std::cmp::Ordering::Greater {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(region.left, region.bottom),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.left, intersection.bottom),
//                     direction: EdgeDirection::Up,
//                 }],
//             });
//         }
//     }
//     if compare_coordinate(region.right, intersection.right) == std::cmp::Ordering::Equal {
//         if compare_coordinate(intersection.top, intersection.bottom) != std::cmp::Ordering::Equal {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.right, intersection.top),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.right, intersection.bottom),
//                     direction: EdgeDirection::Down,
//                 }],
//             });
//         }
//
//         if compare_coordinate(region.top, intersection.top) == std::cmp::Ordering::Less {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(region.right, region.top),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.right, intersection.top),
//                     direction: EdgeDirection::Down,
//                 }],
//             });
//         }
//         if compare_coordinate(region.bottom, intersection.bottom) == std::cmp::Ordering::Greater {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.right, intersection.bottom),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(region.right, region.bottom),
//                     direction: EdgeDirection::Down,
//                 }],
//             });
//         }
//     }
//     if compare_coordinate(region.top, intersection.top) == std::cmp::Ordering::Equal {
//         if compare_coordinate(intersection.left, intersection.right) != std::cmp::Ordering::Equal {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.left, intersection.top),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.right, intersection.top),
//                     direction: EdgeDirection::Right,
//                 }],
//             });
//         }
//
//         if compare_coordinate(region.left, intersection.left) == std::cmp::Ordering::Less {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(region.left, region.top),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.left, intersection.top),
//                     direction: EdgeDirection::Right,
//                 }],
//             });
//         }
//         if compare_coordinate(region.right, intersection.right) == std::cmp::Ordering::Greater {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.right, intersection.top),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(region.right, region.top),
//                     direction: EdgeDirection::Right,
//                 }],
//             });
//         }
//     }
//     if compare_coordinate(region.bottom, intersection.bottom) == std::cmp::Ordering::Equal {
//         if compare_coordinate(intersection.left, intersection.right) != std::cmp::Ordering::Equal {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.right, intersection.bottom),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.left, intersection.bottom),
//                     direction: EdgeDirection::Left,
//                 }],
//             });
//         }
//
//         if compare_coordinate(region.left, intersection.left) == std::cmp::Ordering::Less {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(intersection.left, intersection.bottom),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(region.left, region.bottom),
//                     direction: EdgeDirection::Left,
//                 }],
//             });
//         }
//         if compare_coordinate(region.right, intersection.right) == std::cmp::Ordering::Greater {
//             ret.push(SilihouetteCorner {
//                 pos: Point::new(region.right, region.bottom),
//                 next: vec![NextSilihouetteCorner {
//                     pos: Point::new(intersection.right, intersection.bottom),
//                     direction: EdgeDirection::Left,
//                 }],
//             });
//         }
//     }
//     ret
// }

fn intersect_rects(a: &Rect, b: &Rect) -> Option<Rect> {
    let left = a.left.max(b.left);
    let right = a.right.min(b.right);
    let top = a.top.max(b.top);
    let bottom = a.bottom.min(b.bottom);
    if compare_coordinate(left, right) != std::cmp::Ordering::Greater
        && compare_coordinate(top, bottom) != std::cmp::Ordering::Greater
    {
        Some(Rect::from_ltrb(left, top, right, bottom))
    } else {
        None
    }
}

fn build_collision_corners(regions: &[Rect], i: usize, j: usize) -> Vec<Point> {
    if i != j {
        if let Some(intersection) = intersect_rects(&regions[i], &regions[j]) {
            return build_rect_corners(&intersection);
        }
    }
    vec![]
}

#[derive(Debug, Clone, Copy)]
struct NextSilihouetteCorner {
    pos: Point,
    direction: EdgeDirection,
}

#[derive(Debug, Clone, Copy, Default)]
struct NextCorners {
    used: bool,
    nexts: [Option<NextSilihouetteCorner>; 4],
}

fn build_silhouette_corners(
    regions: &[Rect],
) -> (HashMap<PointWrapper, NextCorners>, PointWrapper) {
    let mut points_in_row = HashMap::new();
    let mut points_in_col = HashMap::new();

    (0..regions.len())
        .cartesian_product(0..regions.len())
        .flat_map(|(i, j)| build_collision_corners(regions, i, j))
        .for_each(|p| {
            points_in_col
                .entry(CoordinateWrapper(p.x))
                .or_insert_with(Vec::new)
                .push(CoordinateWrapper(p.y));
            points_in_row
                .entry(CoordinateWrapper(p.y))
                .or_insert_with(Vec::new)
                .push(CoordinateWrapper(p.x));
        });

    (0..regions.len())
        .flat_map(|i| build_rect_corners(&regions[i]))
        .for_each(|p| {
            points_in_col
                .entry(CoordinateWrapper(p.x))
                .or_insert_with(Vec::new)
                .push(CoordinateWrapper(p.y));
            points_in_row
                .entry(CoordinateWrapper(p.y))
                .or_insert_with(Vec::new)
                .push(CoordinateWrapper(p.x));
        });
    let mut points_in_col = points_in_col
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().unique().collect::<Vec<_>>()))
        .collect::<HashMap<_, _>>();
    let mut points_in_row = points_in_row
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().unique().collect::<Vec<_>>()))
        .collect::<HashMap<_, _>>();

    for (_, v) in points_in_col.iter_mut() {
        v.sort_unstable();
    }
    for (_, v) in points_in_row.iter_mut() {
        v.sort_unstable();
    }

    let mut points = HashMap::new();
    let mut top_left_points = vec![];

    for region in regions {
        let left = CoordinateWrapper(region.left);
        let right = CoordinateWrapper(region.right);
        let top = CoordinateWrapper(region.top);
        let bottom = CoordinateWrapper(region.bottom);

        top_left_points.push(PointWrapper(Point::new(left.0, top.0)));

        if let Some(top_row) = points_in_row.get(&top) {
            let range_start = top_row.binary_search(&left).unwrap();
            let range_end = top_row.binary_search(&right).unwrap();
            for i in range_start..range_end {
                let point = PointWrapper(Point::new(top_row[i].0, top.0));
                let next = points.entry(point).or_insert_with(NextCorners::default);
                next.nexts[EdgeDirection::Right as usize] = Some(NextSilihouetteCorner {
                    pos: Point::new(top_row[i + 1].0, top.0),
                    direction: EdgeDirection::Right,
                });
            }
        }

        if let Some(bottom_row) = points_in_row.get(&bottom) {
            let range_start = bottom_row.binary_search(&left).unwrap();
            let range_end = bottom_row.binary_search(&right).unwrap();
            for i in (range_start..range_end).rev() {
                let point = PointWrapper(Point::new(bottom_row[i + 1].0, bottom.0));
                let next = points.entry(point).or_insert_with(NextCorners::default);
                next.nexts[EdgeDirection::Left as usize] = Some(NextSilihouetteCorner {
                    pos: Point::new(bottom_row[i].0, bottom.0),
                    direction: EdgeDirection::Left,
                });
            }
        }

        if let Some(left_col) = points_in_col.get(&left) {
            let range_start = left_col.binary_search(&top).unwrap();
            let range_end = left_col.binary_search(&bottom).unwrap();
            for i in (range_start..range_end).rev() {
                let point = PointWrapper(Point::new(left.0, left_col[i + 1].0));
                let next = points.entry(point).or_insert_with(NextCorners::default);
                next.nexts[EdgeDirection::Up as usize] = Some(NextSilihouetteCorner {
                    pos: Point::new(left.0, left_col[i].0),
                    direction: EdgeDirection::Up,
                });
            }
        }

        if let Some(right_col) = points_in_col.get(&right) {
            let range_start = right_col.binary_search(&top).unwrap();
            let range_end = right_col.binary_search(&bottom).unwrap();
            for i in range_start..range_end {
                let point = PointWrapper(Point::new(right.0, right_col[i].0));
                let next = points.entry(point).or_insert_with(NextCorners::default);
                next.nexts[EdgeDirection::Down as usize] = Some(NextSilihouetteCorner {
                    pos: Point::new(right.0, right_col[i + 1].0),
                    direction: EdgeDirection::Down,
                });
            }
        }
    }

    top_left_points.sort_unstable();
    let start = top_left_points[0];

    (points, start)
}

fn build_slihouette(regions: &[Rect]) -> (Path, Rect) {
    let mut builder = PathBuilder::new();

    let (mut corners, start) = build_silhouette_corners(regions);
    let points = sort_corsers_in_clockwise_order(start, &mut corners);

    builder.move_to(points[0]);
    for point in points.iter().skip(1) {
        builder.line_to(*point);
    }
    builder.line_to(points[0]);
    builder.close();

    log::debug!("corners: {:?}, points: {:?}", corners, points);

    (builder.snapshot(), builder.compute_bounds())
}

fn sort_corsers_in_clockwise_order(
    start: PointWrapper,
    corners: &mut HashMap<PointWrapper, NextCorners>,
) -> Vec<Point> {
    let mut ret = vec![];
    let mut pivot = Some(start);
    let mut direction = EdgeDirection::Up;
    'pivot: while let Some(current_pos) = pivot {
        if let Some(current) = corners.get_mut(&current_pos) {
            if current.used {
                break;
            }
            if !ret.is_empty() {
                current.used = true;
            }
            ret.push(current_pos.0);

            let try_directions = NEXT_DIRECTION_SEQ[direction as usize];
            for next_direction in try_directions {
                if let Some(next) = current.nexts[next_direction as usize] {
                    pivot = Some(PointWrapper(next.pos));
                    direction = next_direction;
                    continue 'pivot;
                }
            }
        } else {
            break;
        }
    }
    ret
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum EdgeDirection {
    Right = 0,
    Down = 1,
    Left = 2,
    Up = 3,
}

const NEXT_DIRECTION_SEQ: [[EdgeDirection; 4]; 4] = [
    // from Right
    [
        EdgeDirection::Up,
        EdgeDirection::Right,
        EdgeDirection::Down,
        EdgeDirection::Left,
    ],
    // from Down
    [
        EdgeDirection::Right,
        EdgeDirection::Down,
        EdgeDirection::Left,
        EdgeDirection::Up,
    ],
    // from Left
    [
        EdgeDirection::Down,
        EdgeDirection::Left,
        EdgeDirection::Up,
        EdgeDirection::Right,
    ],
    // from Up
    [
        EdgeDirection::Left,
        EdgeDirection::Up,
        EdgeDirection::Right,
        EdgeDirection::Down,
    ],
];

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
        let (mut corners, start) = build_silhouette_corners(&regions);
        let ret = sort_corsers_in_clockwise_order(start, &mut corners);

        println!("{:?}", corners);

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
                Point::new(100., 100.),
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
        let (mut corners, start) = build_silhouette_corners(&regions);
        let ret = sort_corsers_in_clockwise_order(start, &mut corners);

        for corner in corners.iter() {
            println!("{:?}", corner);
        }

        assert_eq!(
            ret,
            vec![
                Point::new(0., 834.),
                Point::new(3420., 834.),
                Point::new(3420., 886.),
                Point::new(3420., 912.),
                Point::new(3420., 1328.),
                Point::new(1692., 1328.),
                Point::new(0., 1328.),
                Point::new(0., 912.),
                Point::new(0., 886.),
                Point::new(0., 834.),
            ]
        );
    }

    #[test]
    fn test_clockwise_paths_case2() {
        let regions = vec![
            Rect::from_ltrb(0., 0., 1000., 500.),
            Rect::from_ltrb(0., 500., 100., 520.),
        ];
        let (mut corners, start) = build_silhouette_corners(&regions);
        for corner in corners.iter() {
            println!("{:?}", corner);
        }

        let ret = sort_corsers_in_clockwise_order(start, &mut corners);

        println!("{:?}", ret);

        assert_eq!(
            ret,
            vec![
                Point::new(0., 0.),
                Point::new(1000., 0.),
                Point::new(1000., 500.),
                Point::new(100., 500.),
                Point::new(100., 520.),
                Point::new(0., 520.),
                Point::new(0., 500.),
                Point::new(0., 0.),
            ]
        );
    }
}
