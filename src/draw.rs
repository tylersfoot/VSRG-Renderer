use macroquad::{color::Color, prelude::*};

pub trait Draw {
    fn draw_rectangle(&mut self, x: f64, y: f64, w: f64, h: f64, color: Color);
    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64, color: Color);
    fn draw_circle(&mut self, x: f64, y: f64, radius: f64, color: Color);
    fn draw_circle_outline(&mut self, x: f64, y: f64, radius: f64, thickness: f64, color: Color);
    // fn draw_text(&mut self, text: &str, x: f64, y: f64, size: f64, color: macroquad::color::Color);
    fn draw_texture(&mut self, texture: &Texture2D, x: f64, y: f64, color: Color);
    fn screen_height(&self) -> f64;
    fn screen_width(&self) -> f64;
}

pub struct MacroquadDraw;

impl Draw for MacroquadDraw {
    fn draw_rectangle(&mut self, x: f64, y: f64, w: f64, h: f64, color: Color) {
        draw_rectangle(x as f32, y as f32, w as f32, h as f32, color);
    }
    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64, color: Color) {
        draw_line(x1 as f32, y1 as f32, x2 as f32, y2 as f32, thickness as f32, color);
    }
    fn draw_circle(&mut self, x: f64, y: f64, radius: f64, color: Color) {
        draw_circle(x as f32, y as f32, radius as f32, color);
    }
    fn draw_circle_outline(&mut self, x: f64, y: f64, radius: f64, thickness: f64, color: Color) {
        draw_circle_lines(x as f32, y as f32, radius as f32, thickness as f32, color);
    }
    // fn draw_text(&mut self, text: &str, x: f64, y: f64, size: f64, color: macroquad::color::Color) {
    //     macroquad::text::draw_text(text, x as f32, y as f32, size as f32, color);
    // }
    fn draw_texture(&mut self, texture: &Texture2D, x: f64, y: f64, color: Color) {
        draw_texture(texture, x as f32, y as f32, color);
    }
    fn screen_height(&self) -> f64 {
        f64::from(screen_height())
    }
    fn screen_width(&self) -> f64 {
        f64::from(screen_width())
    }
}
