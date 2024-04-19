use std::collections::{BTreeMap, HashSet};

use indoc::indoc;
use lazy_static::lazy_static;
use skia_safe::{Point, Rect};

lazy_static! {
    static ref IGNOREABLE_CHARACTERS: HashSet<char> =
        ['-', '|', '*', '+', ' '].iter().cloned().collect();
}

/// Helper function to convert ascii art into a list of rectangles.
/// Each rectangle must have at least two corners labeled in opposite corners to work
/// properly.
/// NOTE: This function returns rectangles that CONTAIN the labels. So when using this function in
/// conjunction with ascii_to_points, you may need to push the outer corners out by one index to
/// get the expected results
/// Returns rects in order by label.
pub fn ascii_to_rects(ascii: &str) -> Vec<Rect> {
    // Split the ascii into lines and characters. Loop over the characters building
    // a list of coordinates by character. After all the text is iterated over, find the min
    // and max coordinate for each and return them as rects.
    let mut points_by_label = BTreeMap::new();
    for (y, line) in ascii.lines().enumerate() {
        for (x, c) in line.chars().enumerate() {
            if IGNOREABLE_CHARACTERS.contains(&c) {
                continue;
            }
            points_by_label
                .entry(c)
                .or_insert_with(Vec::new)
                .push((x, y));
        }
    }

    let mut rects = vec![];
    for points in points_by_label.values() {
        let min_x = points.iter().map(|(x, _)| x).min().unwrap();
        let min_y = points.iter().map(|(_, y)| y).min().unwrap();
        let max_x = points.iter().map(|(x, _)| x).max().unwrap();
        let max_y = points.iter().map(|(_, y)| y).max().unwrap();

        rects.push(Rect::from_xywh(
            *min_x as f32,
            *min_y as f32,
            (max_x - min_x + 1) as f32,
            (max_y - min_y + 1) as f32,
        ));
    }

    rects
}

/// Helper function to convert ascii art into a list of points ordered by their label.
pub fn ascii_to_points(ascii: &str) -> Vec<Point> {
    let mut points = BTreeMap::new();
    for (y, line) in ascii.lines().enumerate() {
        for (x, c) in line.chars().enumerate() {
            if IGNOREABLE_CHARACTERS.contains(&c) {
                continue;
            }
            points.entry(c).or_insert_with(Vec::new).push((x, y));
        }
    }

    points
        .values()
        .flat_map(|points| points.iter().map(|(x, y)| Point::new(*x as f32, *y as f32)))
        .collect()
}

pub fn assert_points_eq(actual: Vec<Point>, expected_ascii: &str) {
    let expected = ascii_to_points(expected_ascii);
    if expected.len() != actual.len() || expected.iter().zip(actual.iter()).any(|(a, b)| a != b) {
        let actual_ascii = points_to_ascii(actual);
        panic!(
            indoc! {"
                Points do not match
                Expected:
                {}
                Actual:
                {}
            "},
            expected_ascii, actual_ascii
        );
    }
}

pub fn points_to_ascii(points: Vec<Point>) -> String {
    if points
        .iter()
        .any(|point| point.x.fract() != 0.0 || point.y.fract() != 0.)
    {
        panic!("Points must be integers to render as ascii");
    }

    let line_width = points.iter().map(|p| p.x as usize).max().unwrap() + 1;
    let line_count = points.iter().map(|p| p.y as usize).max().unwrap() + 1;
    let mut ascii = vec![vec![' '; line_width as usize]; line_count as usize];
    let numbers_big_enough = points.len() <= 9;
    let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    for (index, p) in points.iter().enumerate() {
        let char = if numbers_big_enough {
            std::char::from_digit(index as u32, 10).unwrap()
        } else {
            chars.chars().nth(index).expect("Too many points")
        };
        ascii[p.y as usize][p.x as usize] = char;
    }
    ascii
        .iter()
        .map(|line| line.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn ascii_to_rect_works() {
    // Single rect
    assert_eq!(
        ascii_to_rects(indoc! {"
            1---1
            |   |
            1---1
        "}),
        vec![Rect::from_xywh(0., 0., 5., 3.)]
    );

    // Overlapping rects
    assert_eq!(
        ascii_to_rects(indoc! {"
            1---1
            | 2-+-2
            1-+-1 |
              2---2
        "}),
        vec![
            Rect::from_xywh(0., 0., 5., 3.),
            Rect::from_xywh(2., 1., 5., 3.),
        ]
    );

    // Overlapping rects with shared corner
    assert_eq!(
        ascii_to_rects(indoc! {"
            1----1
            |    |
            *--2-1
            |  |
            2--2
        "}),
        vec![
            Rect::from_xywh(0., 0., 6., 3.),
            Rect::from_xywh(0., 2., 4., 3.),
        ]
    );

    // Adjacent rects
    assert_eq!(
        ascii_to_rects(indoc! {"
            1----1
            |    |
            1----1
            2--2
            |  |
            2--2
        "}),
        vec![
            Rect::from_xywh(0., 0., 6., 3.),
            Rect::from_xywh(0., 3., 4., 3.),
        ]
    );
}

#[test]
fn ascii_to_points_works() {
    // Single point
    assert_eq!(
        ascii_to_points(indoc! {"
            1
        "}),
        vec![Point::new(0., 0.)]
    );

    // Rectangle
    assert_eq!(
        ascii_to_points(indoc! {"
            1-2
            | |
            3-4
        "}),
        vec![
            Point::new(0., 0.),
            Point::new(2., 0.),
            Point::new(0., 2.),
            Point::new(2., 2.),
        ]
    );

    // More complicated shape
    assert_eq!(
        ascii_to_points(indoc! {"
            1-2
            | |
            | 3-4
            |   |
            6---5
        "}),
        vec![
            Point::new(0., 0.),
            Point::new(2., 0.),
            Point::new(2., 2.),
            Point::new(4., 2.),
            Point::new(4., 4.),
            Point::new(0., 4.),
        ]
    );
}
