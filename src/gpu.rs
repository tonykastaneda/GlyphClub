//! GPU bootstrap: wgpu device/surface setup and the vello present pipeline
//! (render into an offscreen target, blit to the swapchain). No app state
//! lives here — this is purely "get a Scene onto the screen."

use std::num::NonZeroUsize;
use std::sync::Arc;

use vello::peniko::Color;
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu;
use vello::{AaConfig, Renderer, RendererOptions, Scene};
use winit::window::Window;

pub struct Gpu {
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
}

impl Gpu {
    pub fn new() -> Self {
        Self {
            context: RenderContext::new(),
            renderers: Vec::new(),
        }
    }

    pub fn create_surface(&mut self, window: Arc<Window>) -> RenderSurface<'static> {
        let size = window.inner_size();
        let surface = pollster::block_on(self.context.create_surface(
            window,
            size.width.max(1),
            size.height.max(1),
            wgpu::PresentMode::AutoVsync,
        ))
        .expect("create surface");

        while self.renderers.len() <= surface.dev_id {
            self.renderers.push(None);
        }
        self.renderers[surface.dev_id].get_or_insert_with(|| {
            Renderer::new(
                &self.context.devices[surface.dev_id].device,
                RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::area_only(),
                    num_init_threads: NonZeroUsize::new(1),
                    pipeline_cache: None,
                },
            )
            .expect("create renderer")
        });

        surface
    }

    pub fn resize(&mut self, surface: &mut RenderSurface<'static>, width: u32, height: u32) {
        self.context
            .resize_surface(surface, width.max(1), height.max(1));
    }

    /// Renders `scene` into `surface` and presents it. Returns without
    /// presenting if the surface texture couldn't be acquired this frame
    /// (window occluded/minimized/timed out) — the next redraw retries.
    pub fn render_and_present(
        &mut self,
        surface: &mut RenderSurface<'static>,
        scene: &Scene,
        base_color: Color,
    ) {
        let width = surface.config.width;
        let height = surface.config.height;
        let device_handle = &self.context.devices[surface.dev_id];

        let surface_texture = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return,
        };

        self.renderers[surface.dev_id]
            .as_mut()
            .unwrap()
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                scene,
                &surface.target_view,
                &vello::RenderParams {
                    base_color,
                    width,
                    height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("render");

        let mut encoder =
            device_handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("surface blit"),
                });
        surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &surface.target_view,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        device_handle.queue.submit([encoder.finish()]);
        surface_texture.present();
    }
}
