//! Vector icon paths rasterized once at 3x and cached as image handles.
//! Image rendering keeps small controls reliable across Iced's CPU/GPU backends.
use iced::{
    widget::{image, image::Handle},
    Color, Element,
};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Library,
    Queue,
    Music,
    Play,
    Pause,
    Previous,
    Next,
    Volume,
    Plus,
    Stop,
    Heart,
    HeartFilled,
    Up,
    Down,
    Close,
    Effects,
    Edit,
}
type IconCache = HashMap<(Icon, [u8; 4]), Handle>;
static CACHE: OnceLock<Mutex<IconCache>> = OnceLock::new();
fn rasterize(glyph: Icon, color: [u8; 4]) -> Handle {
    let mut p = tiny_skia::PathBuilder::new();

    let mut line = |points: &[(f32, f32)]| {
        p.move_to(points[0].0, points[0].1);
        for &(x, y) in &points[1..] {
            p.line_to(x, y);
        }
    };
    match glyph {
        Icon::Edit => {
            line(&[
                (4., 16.),
                (15., 5.),
                (19., 9.),
                (8., 20.),
                (4., 20.),
                (4., 16.),
            ]);
            line(&[(13., 7.), (17., 11.)]);
        }
        Icon::Library => {
            for x in [5., 10., 15.] {
                line(&[(x, 4.), (x, 20.)]);
            }
            line(&[(18., 4.), (21., 20.)]);
        }
        Icon::Queue => {
            for y in [6., 12., 18.] {
                line(&[(8., y), (21., y)]);
                line(&[(3., y), (4., y)]);
            }
        }
        Icon::Music => {
            line(&[(9., 17.), (9., 5.), (20., 3.), (20., 15.)]);
            line(&[(9., 8.), (20., 6.)]);
            p.push_circle(6., 18., 3.);
            p.push_circle(17., 16., 3.);
        }
        Icon::Play => {
            line(&[(8., 5.), (19., 12.), (8., 19.), (8., 5.)]);
        }
        Icon::Pause => {
            line(&[(8., 5.), (8., 19.)]);
            line(&[(16., 5.), (16., 19.)]);
        }
        Icon::Previous | Icon::Next => {
            let (a, b, c) = if matches!(glyph, Icon::Next) {
                (6., 17., 19.)
            } else {
                (18., 7., 5.)
            };
            line(&[(a, 5.), (b, 12.), (a, 19.), (a, 5.)]);
            line(&[(c, 5.), (c, 19.)]);
        }
        Icon::Volume => {
            line(&[
                (11., 4.),
                (6., 9.),
                (3., 9.),
                (3., 15.),
                (6., 15.),
                (11., 20.),
                (11., 4.),
            ]);
            line(&[(16., 8.), (19., 12.), (16., 16.)]);
        }
        Icon::Plus => {
            line(&[(5., 12.), (19., 12.)]);
            line(&[(12., 5.), (12., 19.)]);
        }
        Icon::Heart | Icon::HeartFilled => {
            p.move_to(12., 20.);
            p.cubic_to(10., 18., 3., 13., 3., 8.);
            p.cubic_to(3., 3., 9., 2., 12., 7.);
            p.cubic_to(15., 2., 21., 3., 21., 8.);
            p.cubic_to(21., 13., 14., 18., 12., 20.);
            p.close();
        }
        Icon::Up => {
            line(&[(5., 14.), (12., 7.), (19., 14.)]);
        }
        Icon::Down => {
            line(&[(5., 10.), (12., 17.), (19., 10.)]);
        }
        Icon::Effects => {
            line(&[(3., 6.), (21., 6.)]);
            line(&[(3., 12.), (21., 12.)]);
            line(&[(3., 18.), (21., 18.)]);
            p.push_circle(8., 6., 2.);
            p.push_circle(16., 12., 2.);
            p.push_circle(10., 18., 2.);
        }
        Icon::Close => {
            line(&[(6., 6.), (18., 18.)]);
            line(&[(18., 6.), (6., 18.)]);
        }
        Icon::Stop => {
            line(&[(6., 6.), (18., 6.), (18., 18.), (6., 18.), (6., 6.)]);
        }
    }

    let mut pixmap = tiny_skia::Pixmap::new(72, 72).expect("fixed icon dimensions");
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: 1.7,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };
    if let Some(path) = p.finish() {
        if matches!(glyph, Icon::HeartFilled) {
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::from_scale(3., 3.),
                None,
            );
        }
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::from_scale(3.0, 3.0),
            None,
        );
    }
    let bytes: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let p = pixel.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect();
    Handle::from_rgba(72, 72, bytes)
}
pub fn icon<'a, Message: 'a>(glyph: Icon, size: f32, color: Color) -> Element<'a, Message> {
    let rgba = [
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        (color.a * 255.0) as u8,
    ];
    let mut cache = CACHE.get_or_init(Mutex::default).lock().unwrap();
    let key = (glyph, rgba);
    // Theme changes must not retain every accent ever selected.
    if cache.len() >= 256 && !cache.contains_key(&key) {
        cache.clear();
    }
    let handle = cache
        .entry(key)
        .or_insert_with(|| rasterize(glyph, rgba))
        .clone();
    drop(cache);
    image(handle).width(size).height(size).into()
}
