use eframe::egui::{self, Color32, PointerButton, Pos2, Sense, TextureHandle, Vec2};
use eframe::{App, CreationContext, Frame, NativeOptions};
use image::{ImageBuffer, Rgba};
use rfd::FileDialog;
use std::collections::VecDeque;

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

struct PaintApp {
    pixels: Vec<Color32>,
    canvas_width: usize,
    canvas_height: usize,
    texture: Option<TextureHandle>,
    tool: Tool,
    left_color: Color32,
    right_color: Color32,
    drag_color: Option<Color32>,
    brush_size: f32,
    drag_start: Option<Pos2>,
    last_canvas_pos: Option<Pos2>,
    resizing: bool,
    resize_start: Option<Pos2>,
    resize_original_size: Option<(usize, usize)>,
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
            brush_size: 4.0,
            drag_start: None,
            last_canvas_pos: None,
            resizing: false,
            resize_start: None,
            resize_original_size: None,
        };

        app.update_texture(&cc.egui_ctx);
        app
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
                    self.set_pixel(x, y, color);
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
        self.set_pixel(start_x, start_y, color);

        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if nx >= 0
                    && ny >= 0
                    && nx < self.canvas_width as i32
                    && ny < self.canvas_height as i32
                    && self.pixel(nx, ny) == target
                {
                    self.set_pixel(nx, ny, color);
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
                output.put_pixel(
                    x as u32,
                    y as u32,
                    Rgba([pixel.r(), pixel.g(), pixel.b(), pixel.a()]),
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

        self.canvas_width = image.width() as usize;
        self.canvas_height = image.height() as usize;
        self.pixels = image
            .pixels()
            .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect();

        self.update_texture(ctx);
    }

    fn color_button(ui: &mut egui::Ui, color: Color32) -> bool {
        ui.add(
            egui::Button::new("")
                .fill(color)
                .min_size(Vec2::splat(24.0)),
        )
        .clicked()
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
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("File:");

                if ui.button("Open").clicked() {
                    self.open_png(ctx);
                }

                if ui.button("Save").clicked() {
                    self.save_png();
                }

                if ui.button("New").clicked() {
                    self.new_canvas(INITIAL_WIDTH, INITIAL_HEIGHT);
                    self.update_texture(ctx);
                }

                ui.separator();
                ui.label("Tools:");

                let tools = [
                    (Tool::Pencil, "Pencil"),
                    (Tool::Eraser, "Eraser"),
                    (Tool::ColorPicker, "Picker"),
                    (Tool::Fill, "Fill"),
                    (Tool::Line, "Line"),
                    (Tool::Rectangle, "Rectangle"),
                    (Tool::Ellipse, "Ellipse"),
                ];

                for (tool, label) in tools {
                    if ui.selectable_label(self.tool == tool, label).clicked() {
                        self.tool = tool;
                    }
                }

                ui.separator();
                ui.label("Size:");
                ui.add(egui::Slider::new(&mut self.brush_size, 1.0..=64.0).suffix(" px"));

                ui.separator();
                ui.label("Colors:");

                ui.label("L");
                ui.color_edit_button_srgba(&mut self.left_color);

                ui.label("R");
                ui.color_edit_button_srgba(&mut self.right_color);

                for color in [
                    Color32::BLACK,
                    Color32::WHITE,
                    Color32::from_rgb(128, 128, 128),
                    Color32::RED,
                    Color32::from_rgb(255, 128, 0),
                    Color32::YELLOW,
                    Color32::GREEN,
                    Color32::from_rgb(0, 160, 255),
                    Color32::BLUE,
                    Color32::from_rgb(128, 0, 255),
                    Color32::from_rgb(255, 0, 128),
                ] {
                    let response = ui.add(
                        egui::Button::new("")
                            .fill(color)
                            .min_size(Vec2::splat(20.0)),
                    );
                    if response.clicked_by(PointerButton::Primary) {
                        self.left_color = color;
                    }
                    if response.clicked_by(PointerButton::Secondary) {
                        self.right_color = color;
                    }
                }

                ui.separator();
                ui.label(format!(
                    "L: #{:02X}{:02X}{:02X}  R: #{:02X}{:02X}{:02X}",
                    self.left_color.r(),
                    self.left_color.g(),
                    self.left_color.b(),
                    self.right_color.r(),
                    self.right_color.g(),
                    self.right_color.b()
                ));

                ui.separator();
                ui.label(format!("{} × {}", self.canvas_width, self.canvas_height));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let canvas_size = Vec2::new(self.canvas_width as f32, self.canvas_height as f32);
            let max_width = (available.x - RESIZE_HANDLE_SIZE).max(1.0);
            let max_height = (available.y - RESIZE_HANDLE_SIZE).max(1.0);
            let scale = (max_width / canvas_size.x).min(max_height / canvas_size.y).min(1.0);
            let display_size = canvas_size * scale;

            let (response, painter) =
                ui.allocate_painter(display_size, Sense::click_and_drag());

            let rect = response.rect;

            if let Some(texture) = &self.texture {
                painter.image(
                    texture.id(),
                    rect,
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
                            self.drag_start = Some(canvas_pos);
                            self.last_canvas_pos = Some(canvas_pos);
                            self.drag_color = Some(color);

                            match self.tool {
                                Tool::Pencil => self.draw_dot(canvas_pos, color),
                                Tool::Eraser => self.draw_dot(canvas_pos, Color32::WHITE),
                                _ => {}
                            }
                        }
                    }
                }
            };

            begin_drag(PointerButton::Primary, left_color);
            begin_drag(PointerButton::Secondary, right_color);

            for (button, color) in [
                (PointerButton::Primary, self.left_color),
                (PointerButton::Secondary, self.right_color),
            ] {
                if response.clicked_by(button) {
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
                if response.dragged_by(button) {
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
                                        self.draw_line(previous, canvas_pos, Color32::WHITE);
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
            }

            if self.resizing {
                if let (Some(start), Some(original), Some(pointer)) = (
                    self.resize_start,
                    self.resize_original_size,
                    handle_response.interact_pointer_pos(),
                ) {
                    let width = (original.0 as f32 + (pointer.x - start.x) / scale)
                        .round()
                        .max(MIN_CANVAS_SIZE as f32) as usize;
                    let height = (original.1 as f32 + (pointer.y - start.y) / scale)
                        .round()
                        .max(MIN_CANVAS_SIZE as f32) as usize;

                    if width != self.canvas_width || height != self.canvas_height {
                        self.resize_canvas(width, height);
                        self.update_texture(ctx);
                    }
                }

                if handle_response.drag_stopped() {
                    self.resizing = false;
                    self.resize_start = None;
                    self.resize_original_size = None;
                }
            }

            if handle_response.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
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
