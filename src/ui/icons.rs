//! Line icons drawn with the painter: one stroke weight, one colour, no
//! image assets. Each fills a square `rect`.

use eframe::egui::{pos2, vec2, Color32, CornerRadius, Painter, Rect, Shape, Stroke, StrokeKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Drive,
    Card,
    DiskImage,
    Photo,
    Film,
    Document,
    Archive,
    Database,
}

impl Icon {
    /// The icon for a recovered file's extension.
    pub fn for_extension(ext: &str) -> Self {
        match ext {
            "jpg" | "png" | "gif" | "heic" | "avif" => Icon::Photo,
            "mp4" | "mov" | "m4a" | "3gp" => Icon::Film,
            "zip" => Icon::Archive,
            "sqlite" => Icon::Database,
            _ => Icon::Document,
        }
    }
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width();
    let stroke = Stroke::new((s / 18.0).clamp(1.25, 2.5), color);
    // Coordinates below are in a 24×24 grid, scaled into `rect`.
    let p = |x: f32, y: f32| pos2(rect.left() + x / 24.0 * s, rect.top() + y / 24.0 * s);
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(p(x0, y0), p(x1, y1));
    let round = |v: f32| CornerRadius::same((v / 24.0 * s) as u8);
    let outline = |rect: Rect, radius: f32| painter.rect_stroke(rect, round(radius), stroke, StrokeKind::Middle);
    let line = |a: (f32, f32), b: (f32, f32)| painter.line_segment([p(a.0, a.1), p(b.0, b.1)], stroke);

    match icon {
        Icon::Drive => {
            outline(r(2.5, 6.5, 21.5, 17.5), 2.5);
            line((2.5, 13.0), (21.5, 13.0));
            painter.circle_filled(p(18.0, 15.25), s / 24.0 * 0.9, color);
        }
        Icon::Card => {
            // An SD card: a rectangle with one corner cut off.
            let pts = vec![p(9.0, 3.0), p(18.5, 3.0), p(18.5, 21.0), p(5.5, 21.0), p(5.5, 6.5)];
            painter.add(Shape::closed_line(pts, stroke));
            for x in [9.5, 12.0, 14.5] {
                line((x, 5.5), (x, 8.0));
            }
        }
        Icon::DiskImage => {
            document_outline(painter, p(5.5, 2.5), s, stroke);
            painter.circle_stroke(p(12.0, 14.0), s / 24.0 * 4.0, stroke);
            painter.circle_filled(p(12.0, 14.0), s / 24.0 * 0.9, color);
        }
        Icon::Photo => {
            outline(r(3.0, 4.5, 21.0, 19.5), 2.5);
            painter.circle_stroke(p(8.5, 9.5), s / 24.0 * 1.6, stroke);
            painter.add(Shape::line(vec![p(3.5, 17.0), p(9.5, 12.0), p(13.5, 15.5), p(16.0, 13.5), p(20.5, 17.0)], stroke));
        }
        Icon::Film => {
            outline(r(3.0, 5.0, 21.0, 19.0), 2.5);
            let play = vec![p(10.0, 9.0), p(15.5, 12.0), p(10.0, 15.0)];
            painter.add(Shape::convex_polygon(play, color, Stroke::NONE));
        }
        Icon::Document => {
            document_outline(painter, p(5.5, 2.5), s, stroke);
            for y in [11.0, 14.0, 17.0] {
                line((8.5, y), (15.5, y));
            }
        }
        Icon::Archive => {
            outline(r(3.5, 4.0, 20.5, 8.5), 1.5);
            outline(r(4.5, 8.5, 19.5, 20.0), 1.5);
            line((10.0, 12.0), (14.0, 12.0));
        }
        Icon::Database => {
            let (cx, rx, ry) = (12.0, 7.5, 2.5);
            for y in [5.5, 12.0, 18.5] {
                let ellipse = Shape::ellipse_stroke(p(cx, y), vec2(rx, ry) * s / 24.0, stroke);
                painter.add(ellipse);
            }
            line((cx - rx, 5.5), (cx - rx, 18.5));
            line((cx + rx, 5.5), (cx + rx, 18.5));
        }
    }
}

/// A page with a folded top-right corner, 13×19 grid units.
fn document_outline(painter: &Painter, origin: eframe::egui::Pos2, s: f32, stroke: Stroke) {
    let u = s / 24.0;
    let at = |x: f32, y: f32| origin + vec2(x * u, y * u);
    let pts = vec![at(0.0, 0.0), at(8.5, 0.0), at(13.0, 4.5), at(13.0, 19.0), at(0.0, 19.0)];
    painter.add(Shape::closed_line(pts, stroke));
    painter.add(Shape::line(vec![at(8.5, 0.0), at(8.5, 4.5), at(13.0, 4.5)], stroke));
}
