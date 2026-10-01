use eframe::egui::{self, Color32, PointerButton, Pos2, Sense, TextureHandle, Vec2};
use eframe::{App, CreationContext, Frame, NativeOptions};
use image::{ImageBuffer, Rgba};
use rfd::FileDialog;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

const INITIAL_WIDTH: usize = 1000;
const INITIAL_HEIGHT: usize = 700;
const MIN_CANVAS_SIZE: usize = 32;
const RESIZE_HANDLE_SIZE: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Pencil,
    Eraser,
    ColorPicker,
    Fill,
    Line,
    Rectangle,
    Ellipse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlendMode {
    Normal,
    Additive,
    Subtractive,
}

#[derive(Clone)]
struct CanvasState {
    pixels: Vec<Color32>,
    width: usize,
    height: usize,
}

struct PaintApp {
    pixels: Vec<Color32>,
    canvas_width: usize,
    canvas_height: usize,
    texture: Option<TextureHandle>,
    tool: Tool,
    left_color: Color32,
    right_color: Color32,
    drag_color: Option<Color32>,
    blend_mode: BlendMode,
    brush_size: f32,
    zoom: f32,
    undo_stack: Vec<CanvasState>,
    redo_stack: Vec<CanvasState>,
    drag_start: Option<Pos2>,
    last_canvas_pos: Option<Pos2>,
    resizing: bool,
    resize_start: Option<Pos2>,
    resize_original_size: Option<(usize, usize)>,
    resize_preview_size: Option<(usize, usize)>,
    custom_colors: Vec<Color32>,
    custom_selected: Option<usize>,
    cursor_pos: Option<Pos2>,
}

impl PaintApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        let mut app = Self {
            pixels: vec![Color32::WHITE; INITIAL_WIDTH * INITIAL_HEIGHT],
            canvas_width: INITIAL_WIDTH,
            canvas_height: INITIAL_HEIGHT,
            texture: None,
            tool: Tool::Pencil,
            left_color: Color32::BLACK,
            right_color: Color32::WHITE,
            drag_color: None,
            blend_mode: BlendMode::Normal,
            brush_size: 4.0,
            zoom: 1.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            drag_start: None,
            last_canvas_pos: None,
            resizing: false,
            resize_start: None,
            resize_original_size: None,
            resize_preview_size: None,
            custom_colors: Self::load_custom_colors(),
            custom_selected: None,
            cursor_pos: None,
        };

        app.update_texture(&cc.egui_ctx);
        app
    }

    fn custom_colors_path() -> PathBuf {
        let config_dir = if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        } else if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
            PathBuf::from(path)
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(".config")
        } else {
            PathBuf::from(".")
        };

        config_dir.join("SloppyPaint").join("colors.txt")
    }

    fn load_custom_colors() -> Vec<Color32> {
        let path = Self::custom_colors_path();
        let Ok(contents) = fs::read_to_string(path) else {
            return Vec::new();
        };

        contents.lines().filter_map(Self::parse_hex_color).collect()
    }

    fn parse_hex_color(value: &str) -> Option<Color32> {
        let value = value.trim().trim_start_matches('#');

        if value.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&value[0..2], 16).ok()?;
        let g = u8::from_str_radix(&value[2..4], 16).ok()?;
        let b = u8::from_str_radix(&value[4..6], 16).ok()?;

        Some(Color32::from_rgb(r, g, b))
    }

    fn save_custom_colors(&self) {
        let path = Self::custom_colors_path();

        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                eprintln!("Could not create color settings directory: {error}");
                return;
            }
        }

        let contents = self
            .custom_colors
            .iter()
            .map(|color| format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b()))
            .collect::<Vec<_>>()
            .join("\n");

        if let Err(error) = fs::write(path, format!("{contents}\n")) {
            eprintln!("Could not save custom colors: {error}");
        }
    }

    fn add_custom_color(&mut self, color: Color32) {
        if !self.custom_colors.contains(&color) {
            self.custom_colors.push(color);
            self.custom_selected = Some(self.custom_colors.len() - 1);
            self.save_custom_colors();
        }
    }

    fn delete_selected_custom_color(&mut self) {
        if let Some(index) = self.custom_selected {
            if index < self.custom_colors.len() {
                self.custom_colors.remove(index);
                self.custom_selected = if self.custom_colors.is_empty() {
                    None
                } else {
                    Some(index.min(self.custom_colors.len() - 1))
                };
                self.save_custom_colors();
            }
        }
    }

    fn clear_custom_colors(&mut self) {
        self.custom_colors.clear();
        self.custom_selected = None;
        self.save_custom_colors();
    }

    fn update_texture(&mut self, ctx: &egui::Context) {
        let image = egui::ColorImage::new(
            [self.canvas_width, self.canvas_height],
            self.pixels.clone(),
        );

        if let Some(texture) = &mut self.texture {
            texture.set(image, egui::TextureOptions::NEAREST);
        } else {
            self.texture = Some(ctx.load_texture(
                "paint-canvas",
                image,
                egui::TextureOptions::NEAREST,
            ));
        }
    }

    fn canvas_state(&self) -> CanvasState {
        CanvasState {
            pixels: self.pixels.clone(),
            width: self.canvas_width,
            height: self.canvas_height,
        }
    }

    fn begin_history(&mut self) {
        self.undo_stack.push(self.canvas_state());
        if self.undo_stack.len() > 32 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn restore_state(&mut self, state: CanvasState) {
        self.pixels = state.pixels;
        self.canvas_width = state.width;
        self.canvas_height = state.height;
    }

    fn undo(&mut self) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            return false;
        };

        self.redo_stack.push(self.canvas_state());
        self.restore_state(previous);
        true
    }

    fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };

        self.undo_stack.push(self.canvas_state());
        self.restore_state(next);
        true
    }

    fn clear(&mut self) {
        self.pixels.fill(Color32::WHITE);
    }

    fn new_canvas(&mut self, width: usize, height: usize) {
        self.canvas_width = width.max(MIN_CANVAS_SIZE);
        self.canvas_height = height.max(MIN_CANVAS_SIZE);
        self.pixels = vec![Color32::WHITE; self.canvas_width * self.canvas_height];
    }

    fn resize_canvas(&mut self, width: usize, height: usize) {
        let width = width.max(MIN_CANVAS_SIZE);
        let height = height.max(MIN_CANVAS_SIZE);

        let mut new_pixels = vec![Color32::WHITE; width * height];
        let copy_width = self.canvas_width.min(width);
        let copy_height = self.canvas_height.min(height);

        for y in 0..copy_height {
            let old_start = y * self.canvas_width;
            let new_start = y * width;
            new_pixels[new_start..new_start + copy_width]
                .copy_from_slice(&self.pixels[old_start..old_start + copy_width]);
        }

        self.canvas_width = width;
        self.canvas_height = height;
        self.pixels = new_pixels;
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color32) {
        if x >= 0
            && y >= 0
            && x < self.canvas_width as i32
            && y < self.canvas_height as i32
        {
            self.pixels[y as usize * self.canvas_width + x as usize] = color;
        }
    }

    fn pixel(&self, x: i32, y: i32) -> Color32 {
        self.pixels[y as usize * self.canvas_width + x as usize]
    }

    fn blend_colors(&self, behind: Color32, on_top: Color32) -> Color32 {
        let [br, bg, bb, ba] = behind.to_srgba_unmultiplied();
        let [tr, tg, tb, ta] = on_top.to_srgba_unmultiplied();

        match self.blend_mode {
            BlendMode::Normal => {
                // Standard source-over alpha compositing.
                let source_alpha = ta as f32 / 255.0;
                let behind_alpha = ba as f32 / 255.0;
                let out_alpha = source_alpha + behind_alpha * (1.0 - source_alpha);

                if out_alpha <= 0.0 {
                    return Color32::TRANSPARENT;
                }

                let r = (tr as f32 * source_alpha
                    + br as f32 * behind_alpha * (1.0 - source_alpha))
                    / out_alpha;
                let g = (tg as f32 * source_alpha
                    + bg as f32 * behind_alpha * (1.0 - source_alpha))
                    / out_alpha;
                let b = (tb as f32 * source_alpha
                    + bb as f32 * behind_alpha * (1.0 - source_alpha))
                    / out_alpha;

                Color32::from_rgba_unmultiplied(
                    r.round().clamp(0.0, 255.0) as u8,
                    g.round().clamp(0.0, 255.0) as u8,
                    b.round().clamp(0.0, 255.0) as u8,
                    (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
                )
            }
            BlendMode::Additive => {
                // Add the brush's visible contribution instead of subtracting
                // from the existing pixel. Alpha controls how much is added.
                let strength = ta as u16;
                let r = br as u16 + (tr as u16 * strength / 255);
                let g = bg as u16 + (tg as u16 * strength / 255);
                let b = bb as u16 + (tb as u16 * strength / 255);
                let a = (ba as u16).max(ta as u16);

                Color32::from_rgba_unmultiplied(
                    r.min(255) as u8,
                    g.min(255) as u8,
                    b.min(255) as u8,
                    a.min(255) as u8,
                )
            }
            BlendMode::Subtractive => {
                // Subtractive mode is pigment-style mixing, not arithmetic
                // subtraction. Each RGB channel represents light that remains
                // after the two colors are mixed, so the darker contribution
                // wins for each channel.
                let r = br.min(tr);
                let g = bg.min(tg);
                let b = bb.min(tb);

                let source_alpha = ta as f32 / 255.0;
                let behind_alpha = ba as f32 / 255.0;
                let out_alpha = source_alpha + behind_alpha * (1.0 - source_alpha);

                Color32::from_rgba_unmultiplied(
                    r,
                    g,
                    b,
                    (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
                )
            }
        }
    }

    fn paint_pixel(&mut self, x: i32, y: i32, color: Color32) {
        if x >= 0
            && y >= 0
            && x < self.canvas_width as i32
            && y < self.canvas_height as i32
        {
            let index = y as usize * self.canvas_width + x as usize;
            let existing = self.pixels[index];
            self.pixels[index] = self.blend_colors(existing, color);
        }
    }

    fn erase_pixel(&mut self, x: i32, y: i32) {
        if x >= 0
            && y >= 0
            && x < self.canvas_width as i32
            && y < self.canvas_height as i32
        {
            self.pixels[y as usize * self.canvas_width + x as usize] = Color32::TRANSPARENT;
        }
    }

    fn draw_dot(&mut self, center: Pos2, color: Color32) {
        let radius = self.brush_size / 2.0;
        let min_x = (center.x - radius - 1.0).floor() as i32;
        let max_x = (center.x + radius + 1.0).ceil() as i32;
        let min_y = (center.y - radius - 1.0).floor() as i32;
        let max_y = (center.y + radius + 1.0).ceil() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;

                if dx * dx + dy * dy <= radius * radius {
                    self.paint_pixel(x, y, color);
                }
            }
        }
    }

    fn erase_dot(&mut self, center: Pos2) {
        let radius = self.brush_size / 2.0;
        let min_x = (center.x - radius - 1.0).floor() as i32;
        let max_x = (center.x + radius + 1.0).ceil() as i32;
        let min_y = (center.y - radius - 1.0).floor() as i32;
        let max_y = (center.y + radius + 1.0).ceil() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;

                if dx * dx + dy * dy <= radius * radius {
                    self.erase_pixel(x, y);
                }
            }
        }
    }

    fn draw_line(&mut self, from: Pos2, to: Pos2, color: Color32) {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let distance = dx.hypot(dy);
        let steps = distance.ceil().max(1.0) as usize;

        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let point = Pos2::new(from.x + dx * t, from.y + dy * t);
            self.draw_dot(point, color);
        }
    }

    fn erase_line(&mut self, from: Pos2, to: Pos2) {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let distance = dx.hypot(dy);
        let steps = distance.ceil().max(1.0) as usize;

        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let point = Pos2::new(from.x + dx * t, from.y + dy * t);
            self.erase_dot(point);
        }
    }

    fn draw_shape(&mut self, start: Pos2, end: Pos2, color: Color32) {
        match self.tool {
            Tool::Line => self.draw_line(start, end, color),
            Tool::Rectangle => self.draw_rectangle(start, end, color),
            Tool::Ellipse => self.draw_ellipse(start, end, color),
            _ => {}
        }
    }

    fn draw_rectangle(&mut self, start: Pos2, end: Pos2, color: Color32) {
        let left = start.x.min(end.x);
        let right = start.x.max(end.x);
        let top = start.y.min(end.y);
        let bottom = start.y.max(end.y);

        self.draw_line(Pos2::new(left, top), Pos2::new(right, top), color);
        self.draw_line(Pos2::new(right, top), Pos2::new(right, bottom), color);
        self.draw_line(Pos2::new(right, bottom), Pos2::new(left, bottom), color);
        self.draw_line(Pos2::new(left, bottom), Pos2::new(left, top), color);
    }

    fn draw_ellipse(&mut self, start: Pos2, end: Pos2, color: Color32) {
        let center = Pos2::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
        let radius_x = (end.x - start.x).abs() / 2.0;
        let radius_y = (end.y - start.y).abs() / 2.0;

        if radius_x < 0.5 || radius_y < 0.5 {
            self.draw_dot(center, color);
            return;
        }

        let circumference = (radius_x + radius_y) * std::f32::consts::PI;
        let steps = (circumference * 2.0).ceil().max(16.0) as usize;

        let mut previous = Pos2::new(center.x + radius_x, center.y);

        for i in 1..=steps {
            let angle = std::f32::consts::TAU * i as f32 / steps as f32;
            let point = Pos2::new(
                center.x + radius_x * angle.cos(),
                center.y + radius_y * angle.sin(),
            );
            self.draw_line(previous, point, color);
            previous = point;
        }
    }

    fn flood_fill(&mut self, start_x: i32, start_y: i32, color: Color32) {
        if start_x < 0
            || start_y < 0
            || start_x >= self.canvas_width as i32
            || start_y >= self.canvas_height as i32
        {
            return;
        }

        let target = self.pixel(start_x, start_y);
        if target == color {
            return;
        }

        let mut queue = VecDeque::new();
        queue.push_back((start_x, start_y));
        self.paint_pixel(start_x, start_y, color);

        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if nx >= 0
                    && ny >= 0
                    && nx < self.canvas_width as i32
                    && ny < self.canvas_height as i32
                    && self.pixel(nx, ny) == target
                {
                    self.paint_pixel(nx, ny, color);
                    queue.push_back((nx, ny));
                }
            }
        }
    }

    fn canvas_position(rect: egui::Rect, pointer: Pos2, width: usize, height: usize) -> Option<Pos2> {
        if !rect.contains(pointer) {
            return None;
        }

        Some(Pos2::new(
            (pointer.x - rect.left()) * width as f32 / rect.width(),
            (pointer.y - rect.top()) * height as f32 / rect.height(),
        ))
    }

    fn canvas_to_screen(rect: egui::Rect, point: Pos2, width: usize, height: usize) -> Pos2 {
        Pos2::new(
            rect.left() + point.x * rect.width() / width as f32,
            rect.top() + point.y * rect.height() / height as f32,
        )
    }

    fn save_png(&self) {
        let Some(path) = FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name("untitled.png")
            .save_file()
        else {
            return;
        };

        let mut output =
            ImageBuffer::<Rgba<u8>, Vec<u8>>::new(
                self.canvas_width as u32,
                self.canvas_height as u32,
            );

        for y in 0..self.canvas_height {
            for x in 0..self.canvas_width {
                let pixel = self.pixels[y * self.canvas_width + x];
                let [r, g, b, a] = pixel.to_srgba_unmultiplied();
                output.put_pixel(
                    x as u32,
                    y as u32,
                    Rgba([r, g, b, a]),
                );
            }
        }

        if let Err(error) = output.save(&path) {
            eprintln!("Could not save PNG: {error}");
        }
    }

    fn open_png(&mut self, ctx: &egui::Context) {
        let Some(path) = FileDialog::new()
            .add_filter("PNG image", &["png"])
            .pick_file()
        else {
            return;
        };

        let image = match image::open(&path) {
            Ok(image) => image.to_rgba8(),
            Err(error) => {
                eprintln!("Could not open PNG: {error}");
                return;
            }
        };

        self.begin_history();
        self.canvas_width = image.width() as usize;
        self.canvas_height = image.height() as usize;
        self.pixels = image
            .pixels()
            .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect();

        self.update_texture(ctx);
    }

    fn color_button(ui: &mut egui::Ui, color: Color32) -> egui::Response {
        ui.add(
            egui::Button::new("")
                .fill(color)
                .min_size(Vec2::splat(24.0)),
        )
    }

    fn draw_shape_preview(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        start: Pos2,
        end: Pos2,
        color: Color32,
    ) {
        let start = Self::canvas_to_screen(rect, start, self.canvas_width, self.canvas_height);
        let end = Self::canvas_to_screen(rect, end, self.canvas_width, self.canvas_height);
        let stroke = egui::Stroke::new(self.brush_size.max(1.0), color);

        match self.tool {
            Tool::Line => {
                painter.line_segment([start, end], stroke);
            }
            Tool::Rectangle => {
                painter.rect_stroke(
                    egui::Rect::from_two_pos(start, end),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            Tool::Ellipse => {
                let center = Pos2::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
                let radius = Vec2::new(
                    (end.x - start.x).abs() / 2.0,
                    (end.y - start.y).abs() / 2.0,
                );
                painter.add(egui::Shape::ellipse_stroke(
                    center,
                    radius,
                    stroke,
                ));
            }
            _ => {}
        }
    }

    fn is_shape_tool(&self) -> bool {
        matches!(self.tool, Tool::Line | Tool::Rectangle | Tool::Ellipse)
    }
}

impl App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        let (undo_pressed, redo_pressed) = ctx.input(|input| {
            (
                input.modifiers.ctrl && input.key_pressed(egui::Key::Z),
                (input.modifiers.ctrl && input.key_pressed(egui::Key::Y))
                    || (input.modifiers.ctrl
                        && input.modifiers.shift
                        && input.key_pressed(egui::Key::Z)),
            )
        });

        if undo_pressed && self.undo() {
            self.update_texture(ctx);
        } else if redo_pressed && self.redo() {
            self.update_texture(ctx);
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("File:");

                    if ui.button("Open").clicked() {
                        self.open_png(ctx);
                    }

                    if ui.button("Save").clicked() {
                        self.save_png();
                    }

                    if ui.button("New").clicked() {
                        self.begin_history();
                        self.new_canvas(INITIAL_WIDTH, INITIAL_HEIGHT);
                        self.update_texture(ctx);
                    }

                    if ui.button("Undo").clicked() && self.undo() {
                        self.update_texture(ctx);
                    }

                    if ui.button("Redo").clicked() && self.redo() {
                        self.update_texture(ctx);
                    }

                    ui.separator();
                    ui.label("Tools:");

                    let tools = [
                        (Tool::Pencil, "✏", "Pencil"),
                        (Tool::Eraser, "⌫", "Eraser"),
                        (Tool::ColorPicker, "🎨", "Color picker"),
                        (Tool::Fill, "🪣", "Fill"),
                        (Tool::Line, "╱", "Line"),
                        (Tool::Rectangle, "▣", "Rectangle"),
                        (Tool::Ellipse, "◯", "Ellipse"),
                    ];

                    for (tool, icon, tooltip) in tools {
                        let response = ui.selectable_label(self.tool == tool, icon);
                        if response.clicked() {
                            self.tool = tool;
                        }
                        response.on_hover_text(tooltip);
                    }

                    ui.separator();
                    ui.label("Size:");
                    ui.add(egui::Slider::new(&mut self.brush_size, 1.0..=64.0).suffix(" px"));

                    ui.separator();
                    ui.label("Blend:");
                    ui.selectable_value(&mut self.blend_mode, BlendMode::Normal, "Normal");
                    ui.selectable_value(&mut self.blend_mode, BlendMode::Additive, "Additive");
                    ui.selectable_value(&mut self.blend_mode, BlendMode::Subtractive, "Subtractive");
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Color 1:");

                    for color in [
                        Color32::BLACK,
                        Color32::WHITE,
                        Color32::from_rgb(128, 128, 128),
                        Color32::from_rgb(192, 192, 192),
                        Color32::from_rgb(128, 0, 0),
                        Color32::RED,
                        Color32::from_rgb(255, 128, 0),
                        Color32::YELLOW,
                        Color32::GREEN,
                        Color32::from_rgb(0, 160, 255),
                        Color32::BLUE,
                        Color32::from_rgb(128, 0, 255),
                    ] {
                        let response = Self::color_button(ui, color);
                        if response.clicked_by(PointerButton::Primary) {
                            self.left_color = color;
                        }
                        if response.clicked_by(PointerButton::Secondary) {
                            self.right_color = color;
                        }
                    }

                    ui.separator();
                    ui.label("Color 2:");

                    for color in [
                        Color32::from_rgb(64, 64, 64),
                        Color32::from_rgb(255, 192, 192),
                        Color32::from_rgb(192, 128, 128),
                        Color32::from_rgb(255, 160, 160),
                        Color32::from_rgb(255, 192, 128),
                        Color32::from_rgb(255, 255, 160),
                        Color32::from_rgb(160, 255, 160),
                        Color32::from_rgb(128, 224, 255),
                        Color32::from_rgb(160, 192, 255),
                        Color32::from_rgb(192, 160, 255),
                        Color32::from_rgb(255, 160, 224),
                        Color32::from_rgb(255, 255, 255),
                    ] {
                        if Self::color_button(ui, color) {
                            self.left_color = color;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Current:");

                    ui.color_edit_button_srgba(&mut self.left_color);
                    ui.color_edit_button_srgba(&mut self.right_color);

                    ui.separator();
                    ui.label("Custom:");

                    egui::ComboBox::from_id_salt("custom_colors")
                        .selected_text(
                            self.custom_selected
                                .and_then(|index| self.custom_colors.get(index))
                                .map(|color| {
                                    format!(
                                        "#{:02X}{:02X}{:02X}",
                                        color.r(),
                                        color.g(),
                                        color.b()
                                    )
                                })
                                .unwrap_or_else(|| "Saved colors".to_owned()),
                        )
                        .show_ui(ui, |ui| {
                            if self.custom_colors.is_empty() {
                                ui.label("No saved colors");
                            } else {
                                for (index, color) in self.custom_colors.clone().into_iter().enumerate() {
                                    let selected = self.custom_selected == Some(index);

                                    ui.horizontal(|ui| {
                                        let swatch = ui.add(
                                            egui::Button::new("")
                                                .fill(color)
                                                .min_size(Vec2::splat(20.0)),
                                        );

                                        let label = ui.selectable_label(
                                            selected,
                                            format!(
                                                "#{:02X}{:02X}{:02X}",
                                                color.r(),
                                                color.g(),
                                                color.b()
                                            ),
                                        );

                                        if swatch.clicked_by(PointerButton::Primary)
                                            || label.clicked_by(PointerButton::Primary)
                                        {
                                            self.custom_selected = Some(index);
                                            self.left_color = color;
                                        }

                                        if swatch.clicked_by(PointerButton::Secondary)
                                            || label.clicked_by(PointerButton::Secondary)
                                        {
                                            self.custom_selected = Some(index);
                                            self.right_color = color;
                                        }
                                    });
                                }
                            }

                            ui.separator();

                            if ui.button("Save current left color").clicked() {
                                self.add_custom_color(self.left_color);
                            }

                            if ui.button("Save current right color").clicked() {
                                self.add_custom_color(self.right_color);
                            }

                            if self.custom_selected.is_some()
                                && ui.button("Delete selected color").clicked()
                            {
                                self.delete_selected_custom_color();
                            }

                            if !self.custom_colors.is_empty()
                                && ui.button("Clear all custom colors").clicked()
                            {
                                self.clear_custom_colors();
                            }
                        });

                    ui.separator();
                    ui.label(format!(
                        "L: #{:02X}{:02X}{:02X}",
                        self.left_color.r(),
                        self.left_color.g(),
                        self.left_color.b()
                    ));
                    ui.label(format!(
                        "R: #{:02X}{:02X}{:02X}",
                        self.right_color.r(),
                        self.right_color.g(),
                        self.right_color.b()
                    ));
                });
            });
        });

        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(30.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(position) = self.cursor_pos {
                        ui.label(format!(
                            "Cursor: {}, {}",
                            position.x.floor() as i32,
                            position.y.floor() as i32
                        ));
                    } else {
                        ui.label("Cursor: —");
                    }

                    ui.separator();
                    ui.label("Selection: —");

                    ui.separator();
                    ui.label(format!(
                        "Canvas: {} × {}",
                        self.canvas_width, self.canvas_height
                    ));

                    ui.separator();

                    if ui.button("−").clicked() {
                        self.zoom = (self.zoom / 1.25).clamp(0.1, 8.0);
                    }

                    if ui.button("100%").clicked() {
                        self.zoom = 1.0;
                    }

                    if ui.button("+").clicked() {
                        self.zoom = (self.zoom * 1.25).clamp(0.1, 8.0);
                    }

                    ui.label(format!("{:.0}%", self.zoom * 100.0));
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (display_width, display_height) =
                        self.resize_preview_size.unwrap_or((self.canvas_width, self.canvas_height));
                    let canvas_size = Vec2::new(display_width as f32, display_height as f32);
                    let scale = self.zoom;
                    let display_size = canvas_size * scale;

                    let (response, painter) =
                        ui.allocate_painter(display_size, Sense::click_and_drag());

            let rect = response.rect;

            self.cursor_pos = response.hover_pos().and_then(|pointer| {
                Self::canvas_position(
                    rect,
                    pointer,
                    self.canvas_width,
                    self.canvas_height,
                )
            });

            let checker_size = (16.0 * scale).max(4.0);
            let cols = (rect.width() / checker_size).ceil() as i32;
            let rows = (rect.height() / checker_size).ceil() as i32;
            for y in 0..rows {
                for x in 0..cols {
                    let tile = egui::Rect::from_min_size(
                        Pos2::new(
                            rect.left() + x as f32 * checker_size,
                            rect.top() + y as f32 * checker_size,
                        ),
                        Vec2::splat(checker_size),
                    );
                    let shade = if (x + y) % 2 == 0 { 230 } else { 200 };
                    painter.rect_filled(tile, 0.0, Color32::from_gray(shade));
                }
            }

            if let Some(texture) = &self.texture {
                let image_rect = egui::Rect::from_min_size(
                    rect.min,
                    Vec2::new(
                        self.canvas_width as f32 * scale,
                        self.canvas_height as f32 * scale,
                    ),
                );
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }

            if response.hovered() && !self.resizing {
                ctx.set_cursor_icon(if self.is_shape_tool() {
                    egui::CursorIcon::Crosshair
                } else {
                    egui::CursorIcon::Crosshair
                });
            }

            let left_color = self.left_color;
            let right_color = self.right_color;

            let mut begin_drag = |button: PointerButton, color: Color32| {
                if response.drag_started_by(button) {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if let Some(canvas_pos) = Self::canvas_position(
                            rect,
                            pointer,
                            self.canvas_width,
                            self.canvas_height,
                        ) {
                            self.begin_history();
                            self.drag_start = Some(canvas_pos);
                            self.last_canvas_pos = Some(canvas_pos);
                            self.drag_color = Some(color);

                            match self.tool {
                                Tool::Pencil => self.draw_dot(canvas_pos, color),
                                Tool::Eraser => self.erase_dot(canvas_pos),
                                _ => {}
                            }
                        }
                    }
                }
            };

            if !self.resizing {
                begin_drag(PointerButton::Primary, left_color);
                begin_drag(PointerButton::Secondary, right_color);
            }

            for (button, color) in [
                (PointerButton::Primary, self.left_color),
                (PointerButton::Secondary, self.right_color),
            ] {
                if !self.resizing && response.clicked_by(button) {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if let Some(canvas_pos) = Self::canvas_position(
                            rect,
                            pointer,
                            self.canvas_width,
                            self.canvas_height,
                        ) {
                            match self.tool {
                                Tool::ColorPicker => {
                                    let x = canvas_pos.x.floor() as i32;
                                    let y = canvas_pos.y.floor() as i32;
                                    if x >= 0
                                        && y >= 0
                                        && x < self.canvas_width as i32
                                        && y < self.canvas_height as i32
                                    {
                                        let picked = self.pixel(x, y);
                                        if button == PointerButton::Primary {
                                            self.left_color = picked;
                                        } else {
                                            self.right_color = picked;
                                        }
                                    }
                                }
                                Tool::Fill => {
                                    self.begin_history();
                                    self.flood_fill(
                                        canvas_pos.x.floor() as i32,
                                        canvas_pos.y.floor() as i32,
                                        color,
                                    );
                                    self.update_texture(ctx);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            for (button, color) in [
                (PointerButton::Primary, self.left_color),
                (PointerButton::Secondary, self.right_color),
            ] {
                if !self.resizing && response.dragged_by(button) {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if let Some(canvas_pos) = Self::canvas_position(
                            rect,
                            pointer,
                            self.canvas_width,
                            self.canvas_height,
                        ) {
                            match self.tool {
                                Tool::Pencil => {
                                    if let Some(previous) = self.last_canvas_pos {
                                        self.draw_line(previous, canvas_pos, color);
                                    }
                                    self.last_canvas_pos = Some(canvas_pos);
                                    self.update_texture(ctx);
                                }
                                Tool::Eraser => {
                                    if let Some(previous) = self.last_canvas_pos {
                                        self.erase_line(previous, canvas_pos);
                                    }
                                    self.last_canvas_pos = Some(canvas_pos);
                                    self.update_texture(ctx);
                                }
                                Tool::Line | Tool::Rectangle | Tool::Ellipse => {}
                                _ => {}
                            }
                        }
                    }
                }
            }

            if response.drag_stopped_by(PointerButton::Primary)
                || response.drag_stopped_by(PointerButton::Secondary)
            {
                if let Some(start) = self.drag_start.take() {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if let Some(end) = Self::canvas_position(
                            rect,
                            pointer,
                            self.canvas_width,
                            self.canvas_height,
                        ) {
                            if self.is_shape_tool() {
                                if let Some(color) = self.drag_color {
                                    self.draw_shape(start, end, color);
                                    self.update_texture(ctx);
                                }
                            }
                        }
                    }
                }
                self.last_canvas_pos = None;
                self.drag_color = None;
            }

            if self.is_shape_tool() {
                if let (Some(start), Some(pointer)) =
                    (self.drag_start, response.hover_pos())
                {
                    if let Some(end) = Self::canvas_position(rect, pointer, self.canvas_width, self.canvas_height) {
                        if let Some(color) = self.drag_color {
                            self.draw_shape_preview(&painter, rect, start, end, color);
                        }
                    }
                }
            }

            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, Color32::GRAY),
                egui::StrokeKind::Outside,
            );

            if let Some(pointer) = response.hover_pos() {
                if let Some(canvas_pos) = Self::canvas_position(rect, pointer, self.canvas_width, self.canvas_height) {
                    let radius = self.brush_size * scale / 2.0;
                    let screen_radius = radius.min(32.0);

                    if matches!(self.tool, Tool::Pencil | Tool::Eraser) {
                        painter.circle_stroke(
                            Self::canvas_to_screen(rect, canvas_pos, self.canvas_width, self.canvas_height),
                            screen_radius.max(1.0),
                            egui::Stroke::new(1.0, Color32::BLACK),
                        );
                    }
                }
            }

            let handle_rect = egui::Rect::from_min_size(
                Pos2::new(rect.right() - RESIZE_HANDLE_SIZE, rect.bottom() - RESIZE_HANDLE_SIZE),
                Vec2::splat(RESIZE_HANDLE_SIZE),
            );

            let handle_response = ui.interact(
                handle_rect,
                ui.id().with("canvas-resize"),
                Sense::drag(),
            );

            painter.rect_filled(handle_rect, 0.0, Color32::from_gray(180));
            painter.line_segment(
                [
                    Pos2::new(handle_rect.left() + 3.0, handle_rect.bottom() - 3.0),
                    Pos2::new(handle_rect.right() - 3.0, handle_rect.top() + 3.0),
                ],
                egui::Stroke::new(1.0, Color32::DARK_GRAY),
            );

            if handle_response.drag_started() {
                self.resizing = true;
                self.resize_start = handle_response.interact_pointer_pos();
                self.resize_original_size = Some((self.canvas_width, self.canvas_height));
                self.resize_preview_size = Some((self.canvas_width, self.canvas_height));
            }

            if self.resizing {
                if let (Some(start), Some(original), Some(pointer)) = (
                    self.resize_start,
                    self.resize_original_size,
                    handle_response.interact_pointer_pos(),
                ) {
                    let width = (original.0 as f32 + (pointer.x - start.x) / self.zoom)
                        .round()
                        .max(MIN_CANVAS_SIZE as f32) as usize;
                    let height = (original.1 as f32 + (pointer.y - start.y) / self.zoom)
                        .round()
                        .max(MIN_CANVAS_SIZE as f32) as usize;

                    self.resize_preview_size = Some((width, height));
                }

                if handle_response.drag_stopped() {
                    if let Some((width, height)) = self.resize_preview_size {
                        if (width, height) != (self.canvas_width, self.canvas_height) {
                            self.begin_history();
                            self.resize_canvas(width, height);
                            self.update_texture(ctx);
                        }
                    }

                    self.resizing = false;
                    self.resize_start = None;
                    self.resize_original_size = None;
                    self.resize_preview_size = None;
                }
            }

            if handle_response.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
                });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Sloppy Paint")
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([600.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Sloppy Paint",
        options,
        Box::new(|cc| Ok(Box::new(PaintApp::new(cc)))),
    )
}
