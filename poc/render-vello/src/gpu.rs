//! Rendering with the GPU, with `vello` on `wgpu`.

use vello::kurbo::{Affine, Stroke};
use vello::peniko::{Fill, Gradient};
use vello::wgpu;
use vello::{AaConfig, AaSupport, Glyph, RenderParams, Renderer, RendererOptions, Scene};

use crate::camera::Camera;
use crate::image::Rgba;
use crate::scene::{BACKGROUND, SceneDesc, TEXT_COLOR};
use crate::text::Font;

/// Format Vello writes into. Values are sRGB-encoded.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Encodes `desc` once, in world units.
pub fn world_scene(desc: &SceneDesc, font: &Font) -> Scene {
    encode(desc, font, Affine::IDENTITY)
}

/// The scene seen through `camera`. Copies the encoded world scene with the
/// view transform, which is much faster than encoding it again every frame.
pub fn view_scene(world: &Scene, camera: &Camera) -> Scene {
    let mut scene = Scene::new();
    scene.append(world, Some(camera.transform()));
    scene
}

/// Encodes every item of `desc` with `view` applied.
pub fn encode(desc: &SceneDesc, font: &Font, view: Affine) -> Scene {
    let mut scene = Scene::new();
    for tile in &desc.tiles {
        let gradient =
            Gradient::new_linear(tile.gradient_start, tile.gradient_end).with_stops(tile.colors);
        scene.fill(Fill::NonZero, view, &gradient, None, &tile.shape);
        scene.stroke(
            &Stroke::new(tile.stroke_width),
            view,
            tile.stroke,
            None,
            &tile.shape,
        );
    }
    for line in &desc.lines {
        let glyphs = font.layout(&line.text, line.size);
        scene
            .draw_glyphs(font.data())
            .font_size(line.size)
            .hint(false)
            .transform(view * Affine::translate(line.origin.to_vec2()))
            .brush(TEXT_COLOR)
            .draw(
                Fill::NonZero,
                glyphs.iter().map(|g| Glyph {
                    id: g.id,
                    x: g.x,
                    y: 0.0,
                }),
            );
    }
    scene
}

/// Vello's GPU renderer, bound to one wgpu device.
pub struct Gpu {
    renderer: Renderer,
}

impl Gpu {
    pub fn new(device: &wgpu::Device) -> Result<Self, vello::Error> {
        let renderer = Renderer::new(
            device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )?;
        Ok(Self { renderer })
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<(), vello::Error> {
        self.renderer.render_to_texture(
            device,
            queue,
            scene,
            target,
            &RenderParams {
                base_color: BACKGROUND,
                width,
                height,
                antialiasing_method: AaConfig::Area,
            },
        )
    }

    /// Renders into a new texture and reads the pixels back.
    #[allow(clippy::too_many_arguments)]
    pub fn render_to_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &Scene,
        camera: &Camera,
        width: u32,
        height: u32,
    ) -> Result<Rgba, String> {
        let texture = target_texture(device, width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let scene = view_scene(world, camera);
        self.render(device, queue, &scene, &view, width, height)
            .map_err(|e| e.to_string())?;
        read_texture(device, queue, &texture)
    }
}

/// A texture Vello can render into and egui can display.
pub fn target_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
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
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Copies an `Rgba8Unorm` texture back to memory.
pub fn read_texture(
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

/// Vello keeps one row per distinct gradient in a texture, so 10,000 distinct
/// gradients need a texture taller than wgpu's default limit of 8192. Ask the
/// adapter for its real maximum (16384 on the GTX 1660).
pub fn device_descriptor(adapter_limits: &wgpu::Limits) -> wgpu::DeviceDescriptor<'static> {
    wgpu::DeviceDescriptor {
        label: Some("bivar poc device"),
        required_limits: wgpu::Limits {
            max_texture_dimension_2d: adapter_limits.max_texture_dimension_2d,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A GPU device without any window, for exports and tests.
pub struct Headless {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
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
        let (device, queue) =
            pollster::block_on(adapter.request_device(&device_descriptor(&adapter.limits())))
                .map_err(|e| e.to_string())?;
        Ok(Self {
            device,
            queue,
            adapter_name,
        })
    }
}
