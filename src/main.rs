use eframe::egui::{self, Color32, Pos2, Sense, TextureHandle, Vec2};
use eframe::{App, CreationContext, Frame, NativeOptions};
use image::{ImageBuffer, Rgba};
use rfd::FileDialog;

const CANVAS_WIDTH: usize = 1000;
const CANVAS_HEIGHT: usize = 700;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Pencil,
    Eraser,
}

struct PaintApp {
    pixels: Vec<Color32>,
    texture: Option<TextureHandle>,
    tool: Tool,
    color: Color32,
    brush_size: f32,
    last_canvas_pos: Option<Pos2>,
}

impl PaintApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        let mut app = Self {
            pixels: vec![Color32::WHITE; CANVAS_WIDTH * CANVAS_HEIGHT],
            texture: None,
            tool: Tool::Pencil,
            color: Color32::BLACK,
            brush_size: 4.0,
            last_canvas_pos: None,
        };

        app.update_texture(&cc.egui_ctx);
        app
    }

    fn update_texture(&mut self, ctx: &egui::Context) {
        let image = egui::ColorImage::new(
            [CANVAS_WIDTH, CANVAS_HEIGHT],
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

    fn set_pixel(&mut self, x: i32, y: i32, color: Color32) {
        if x >= 0
            && y >= 0
            && x < CANVAS_WIDTH as i32
            && y < CANVAS_HEIGHT as i32
        {
            self.pixels[y as usize * CANVAS_WIDTH + x as usize] = color;
        }
    }

    fn draw_dot(&mut self, center: Pos2) {
        let radius = self.brush_size / 2.0;
        let min_x = (center.x - radius - 1.0).floor() as i32;
        let max_x = (center.x + radius + 1.0).ceil() as i32;
        let min_y = (center.y - radius - 1.0).floor() as i32;
        let max_y = (center.y + radius + 1.0).ceil() as i32;

        let draw_color = match self.tool {
            Tool::Pencil => self.color,
            Tool::Eraser => Color32::WHITE,
        };

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;

                if dx * dx + dy * dy <= radius * radius {
                    self.set_pixel(x, y, draw_color);
                }
            }
        }
    }

    fn draw_line(&mut self, from: Pos2, to: Pos2) {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let distance = dx.hypot(dy);
        let steps = distance.ceil().max(1.0) as usize;

        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let point = Pos2::new(from.x + dx * t, from.y + dy * t);
            self.draw_dot(point);
        }
    }

    fn canvas_position(response_rect: egui::Rect, pointer: Pos2) -> Option<Pos2> {
        if !response_rect.contains(pointer) {
            return None;
        }

        let x = (pointer.x - response_rect.left())
            * CANVAS_WIDTH as f32
            / response_rect.width();

        let y = (pointer.y - response_rect.top())
            * CANVAS_HEIGHT as f32
            / response_rect.height();

        Some(Pos2::new(x, y))
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
                CANVAS_WIDTH as u32,
                CANVAS_HEIGHT as u32,
            );

        for y in 0..CANVAS_HEIGHT {
            for x in 0..CANVAS_WIDTH {
                let pixel = self.pixels[y * CANVAS_WIDTH + x];

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

        self.pixels.fill(Color32::WHITE);

        let copy_width = CANVAS_WIDTH.min(image.width() as usize);
        let copy_height = CANVAS_HEIGHT.min(image.height() as usize);

        for y in 0..copy_height {
            for x in 0..copy_width {
                let pixel = image.get_pixel(x as u32, y as u32);

                self.pixels[y * CANVAS_WIDTH + x] =
                    Color32::from_rgba_unmultiplied(
                        pixel[0],
                        pixel[1],
                        pixel[2],
                        pixel[3],
                    );
            }
        }

        self.update_texture(ctx);
    }

    fn color_button(ui: &mut egui::Ui, color: Color32) -> bool {
        let button = egui::Button::new("")
            .fill(color)
            .min_size(Vec2::splat(24.0));

        ui.add(button).clicked()
    }
}

impl App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("File:");

                if ui.button("Open").clicked() {
                    self.open_png(ctx);
                }

                if ui.button("Save").clicked() {
                    self.save_png();
                }

                if ui.button("New").clicked() {
                    self.clear();
                    self.update_texture(ctx);
                }

                ui.separator();

                ui.label("Tool:");

                if ui
                    .selectable_label(self.tool == Tool::Pencil, "Pencil")
                    .clicked()
                {
                    self.tool = Tool::Pencil;
                }

                if ui
                    .selectable_label(self.tool == Tool::Eraser, "Eraser")
                    .clicked()
                {
                    self.tool = Tool::Eraser;
                }

                ui.separator();

                ui.label("Size:");
                ui.add(
                    egui::Slider::new(&mut self.brush_size, 1.0..=64.0)
                        .suffix(" px"),
                );

                ui.separator();

                ui.label("Color:");
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
                    if Self::color_button(ui, color) {
                        self.color = color;
                        self.tool = Tool::Pencil;
                    }
                }

                ui.separator();
                ui.label(format!(
                    "{} × {}",
                    CANVAS_WIDTH, CANVAS_HEIGHT
                ));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let canvas_size = Vec2::new(
                available.x.min(CANVAS_WIDTH as f32),
                available.y.min(CANVAS_HEIGHT as f32),
            );

            let (response, painter) =
                ui.allocate_painter(canvas_size, Sense::drag());

            let rect = response.rect;

            if let Some(texture) = &self.texture {
                painter.image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(
                        Pos2::ZERO,
                        Pos2::new(1.0, 1.0),
                    ),
                    Color32::WHITE,
                );
            }

            if response.drag_started() || response.dragged() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    if let Some(canvas_pos) =
                        Self::canvas_position(rect, pointer)
                    {
                        if response.drag_started() {
                            self.draw_dot(canvas_pos);
                        } else if let Some(previous) = self.last_canvas_pos {
                            self.draw_line(previous, canvas_pos);
                        }

                        self.last_canvas_pos = Some(canvas_pos);
                        self.update_texture(ctx);
                    }
                }
            } else {
                self.last_canvas_pos = None;
            }

            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, Color32::GRAY),
                egui::StrokeKind::Outside,
            );

            if let Some(pointer) = response.hover_pos() {
                if let Some(canvas_pos) =
                    Self::canvas_position(rect, pointer)
                {
                    let radius = self.brush_size
                        * rect.width()
                        / CANVAS_WIDTH as f32
                        / 2.0;

                    painter.circle_stroke(
                        pointer,
                        radius.max(1.0),
                        egui::Stroke::new(1.0, Color32::BLACK),
                    );

                    let _ = canvas_pos;
                }
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
