fn compute_text_region(grid_pos: (u64, u64), cell_width: u64, font_width: f32, font_height: f32) -> Rect {
    let (grid_x, grid_y) = grid_pos;
    let x = grid_x as f32 * font_width;
    let y = grid_y as f32 * font_height;
    let width = cell_width as f32 * font_width as f32;
    let height = font_height as f32;
    Rect::new(x, y, x + width, y + height)
}

fn draw_background(
    canvas: &mut Canvas,
    grid_pos: (u64, u64),
    cell_width: u64,
    style: &Option<Arc<Style>>,
    default_style: &Arc<Style>,
    floating: bool,
    settings: &RendererSettings
) {
    self.paint.set_blend_mode(BlendMode::Src);

    let region = self.compute_text_region(grid_pos, cell_width);
    let style = style.as_ref().unwrap_or(default_style);

    let mut color = style.background(&default_style.colors);

    if floating {
        color.a = color.a * settings.floating_opacity.min(1.0).max(0.0);
    }

    self.paint.set_color(color.to_color());
    canvas.draw_rect(region, &self.paint);
}

fn draw_foreground(
    canvas: &mut Canvas,
    text: &str,
    grid_pos: (u64, u64),
    cell_width: u64,
    style: &Option<Arc<Style>>,
    default_style: &Arc<Style>,
) {
    let (grid_x, grid_y) = grid_pos;
    let x = grid_x as f32 * self.font_width;
    let y = grid_y as f32 * self.font_height;
    let width = cell_width as f32 * self.font_width;

    let style = style.as_ref().unwrap_or(default_style);

    canvas.save();

    let region = self.compute_text_region(grid_pos, cell_width);

    canvas.clip_rect(region, None, Some(false));

    if style.underline || style.undercurl {
        let line_position = self.shaper.underline_position();
        let stroke_width = self.shaper.options.size / 10.0;
        self.paint
            .set_color(style.special(&default_style.colors).to_color());
        self.paint.set_stroke_width(stroke_width);

        if style.undercurl {
            self.paint.set_path_effect(dash_path_effect::new(
                &[stroke_width * 2.0, stroke_width * 2.0],
                0.0,
            ));
        } else {
            self.paint.set_path_effect(None);
        }

        canvas.draw_line(
            (x, y - line_position + self.font_height),
            (x + width, y - line_position + self.font_height),
            &self.paint,
        );
    }

    self.paint
        .set_color(style.foreground(&default_style.colors).to_color());
    let text = text.trim_end();
    if !text.is_empty() {
        for blob in self
            .shaper
            .shape_cached(text, style.bold, style.italic)
            .iter()
        {
            canvas.draw_text_blob(blob, (x, y), &self.paint);
        }
    }

    if style.strikethrough {
        let line_position = region.center_y();
        self.paint
            .set_color(style.special(&default_style.colors).to_color());
        canvas.draw_line((x, line_position), (x + width, line_position), &self.paint);
    }

    canvas.restore();
}
