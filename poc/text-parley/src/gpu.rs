//! Rendering with the GPU, with `vello` on `wgpu`, without any window.

use std::sync::Arc;

use vello::kurbo::{Affine, Stroke};
use vello::peniko::{Blob, Fill, FontData};
use vello::wgpu;
use vello::{AaConfig, AaSupport, Glyph, RenderParams, Renderer, RendererOptions, Scene};

use crate::fonts::TestFont;
use crate::image::Rgba;
use crate::page::{BACKGROUND, Page};

/// Format Vello writes into. Values are sRGB-encoded.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub fn font_data(font: TestFont) -> FontData {
    FontData::new(Blob::new(Arc::new(font.bytes())), 0)
}

/// Encodes every item of `page`, in pixels.
pub fn encode(page: &Page) -> Scene {
    let mut scene = Scene::new();
    let fonts = TestFont::ALL.map(|font| (font, font_data(font)));
    for frame in &page.frames {
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            frame.color,
            None,
            &frame.rect,
        );
    }
    for run in &page.runs {
        let (_, data) = fonts.iter().find(|(f, _)| *f == run.font).unwrap();
        let (x, y) = run.origin;
        scene
            .draw_glyphs(data)
            .font_size(run.size)
            .hint(false)
            .transform(Affine::translate((f64::from(x), f64::from(y))))
            .brush(run.color)
            .draw(
                Fill::NonZero,
                run.glyphs.iter().map(|g| Glyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                }),
            );
    }
    scene
}

/// A GPU device and Vello's renderer, without any window.
pub struct Headless {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    pub adapter_name: String,
}

impl Headless {
    pub fn new() -> Result<Self, String> {
        // WGPU_BACKEND=dx12 or vulkan picks the backend, for comparisons.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        let adapter_name = format!("{} ({:?})", info.name, info.backend);
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .map_err(|e| e.to_string())?;
        let renderer = Renderer::new(
            &device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            device,
            queue,
            renderer,
            adapter_name,
        })
    }

    /// Renders `page` into a new texture and reads the pixels back.
    pub fn render(&mut self, page: &Page) -> Result<Rgba, String> {
        let (width, height) = (u32::from(page.width), u32::from(page.height));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vello target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &encode(page),
                &view,
                &RenderParams {
                    base_color: BACKGROUND,
                    width,
                    height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| e.to_string())?;
        read_texture(&self.device, &self.queue, &texture)
    }
}

/// Copies an `Rgba8Unorm` texture back to memory.
fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Rgba, String> {
    let (width, height) = (texture.width(), texture.height());
    let row_bytes = width * 4;
    let padded_row = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(padded_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).ok();
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    receiver
        .recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let mapped = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let mut pixels = Vec::with_capacity(row_bytes as usize * height as usize);
    for row in mapped.chunks_exact(padded_row as usize) {
        pixels.extend_from_slice(&row[..row_bytes as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(Rgba::new(width, height, pixels))
}
