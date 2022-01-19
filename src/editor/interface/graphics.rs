//! Fully handles drawing of custom graphics to the editor window.
//!
//! In this plugin, rendering is achieved with `wgpu`, which provides a very low-level API. This is
//! very flexible, but requires a lot of setup!

use cgmath::{prelude::SquareMatrix, Matrix4, Vector3};
use once_cell::sync::Lazy;
use wgpu::util::DeviceExt;
use wgpu_glyph::{GlyphBrush, GlyphBrushBuilder};
use zerocopy::AsBytes;

use super::{
    image_consts::{ORIG_BG_SIZE_X, ORIG_BG_SIZE_Y, ORIG_KNOB_RADIUS, ORIG_KNOB_X, ORIG_KNOB_Y},
    SCALE, SIZE_X, SIZE_Y,
};

const MSAA_SAMPLES: u32 = 4;

/// Contains all handles to GPU resources required for rendering the editor interface.
pub(super) struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    multisampled_framebuffer: wgpu::TextureView,
    swap_chain: wgpu::SwapChain,

    text_renderer: GlyphBrush<()>,
    /// Required by `wgpu_glyph`
    local_pool: futures::executor::LocalPool,
    /// Required by `wgpu_glyph`
    staging_belt: wgpu::util::StagingBelt,

    pipeline: wgpu::RenderPipeline,
    rectangle_index_buffer: wgpu::Buffer,
    rectangle_vertex_buffer: wgpu::Buffer,

    background_bind_group: wgpu::BindGroup,

    pointer_bind_group: wgpu::BindGroup,
    pointer_transform_buffer: wgpu::Buffer,
}

/// Low-level representation of a point in 3D space. This representation is designed to be shared
/// directly with GPU memory for use in shaders.
#[repr(C)]
#[derive(Clone, Copy, AsBytes)]
struct Vertex {
    /// `[x, y, z, w]` position. For a 2D interface, only `x` and `y` are important.
    _pos: [f32; 4],
    /// `[u, v]` texture coordinate position used to map parts of an image to a piece of geometry.
    _quad_coord: [f32; 2],
}

impl Vertex {
    pub fn new(x: f32, y: f32, u: f32, v: f32) -> Self {
        Vertex {
            _pos: [x, y, 0., 0.],
            _quad_coord: [u, v],
        }
    }
}

/// Low-level representation of a 4x4 matrix for transformations in 3D space. This representation
/// is designed to be shared directly with GPU memory for use in shaders.
#[repr(C)]
#[derive(Clone, Copy, AsBytes)]
struct TransformUniform {
    transform: [[f32; 4]; 4],
}

const BACKGROUND_IMAGE: &[u8] = include_bytes!("../../../assets/images/bg.png");
const POINTER_IMAGE: &[u8] = include_bytes!("../../../assets/images/pointer.png");
const FONT: &[u8] = include_bytes!("../../../assets/fonts/iosevka-Iosevka-medium.ttf");
const FONT_COLOR: [f32; 4] = [1.0, 0.51, 0.0, 1.0];

const TEXT_RIGHT_ANCHOR: f32 = 460. * SCALE as f32;
const TEXT_CENTER_Y_ANCHOR: f32 = 500. * SCALE as f32;

/// Scales and moves the original knob image from ([-1,1],[-1,1]) to its correct position on the
/// background image.
static SCALE_MOVE_KNOB_TRANSFORM: Lazy<Matrix4<f32>> = Lazy::new(|| {
    Matrix4::from_translation(Vector3::new(
        2. * (ORIG_KNOB_X - ORIG_BG_SIZE_X / 2) as f32 / ORIG_BG_SIZE_X as f32,
        2. * -((ORIG_KNOB_Y - ORIG_BG_SIZE_Y / 2) as f32) / ORIG_BG_SIZE_Y as f32,
        0.,
    )) * Matrix4::from_nonuniform_scale(
        (ORIG_KNOB_RADIUS * 2) as f32 / ORIG_BG_SIZE_X as f32,
        (ORIG_KNOB_RADIUS * 2) as f32 / ORIG_BG_SIZE_Y as f32,
        1.,
    )
});

impl Renderer {
    /// Creates a new `Renderer` by initializing the GPU to prepare it for rendering.
    pub fn new<W: raw_window_handle::HasRawWindowHandle>(handle: W) -> Self {
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);

        // Acquire the window as a surface to be rendered on.
        // This is the only unsafe code in the plugin; it is only required to satisfy the
        // `raw_window_handle` API. Safety is upheld by taking ownership of `handle` in the
        // function signature, ensuring it is only ever used to create a single surface.
        let surface = unsafe { instance.create_surface(&handle) };

        // Get a handle to the GPU and a queue of commands to be uploaded to it while rendering.
        let (device, queue) = futures::executor::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                })
                .await
                .unwrap();

            adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: None,
                        features: wgpu::Features::empty(),
                        limits: wgpu::Limits::default(),
                    },
                    None,
                )
                .await
                .unwrap()
        });

        // Shaders are written in GLSL and compiled to SPIR-V from `build.rs`. They describe how
        // to layout points in space (vertex shaders), or how to render triangular fragments to
        // the screen (fragment shaders). The resulting SPIR-V is loaded to the GPU at runtime.
        let vs_module = device.create_shader_module(&wgpu::include_spirv!(
            "../../../assets/generated/spirv/shader.vert.spv"
        ));
        let fs_module = device.create_shader_module(&wgpu::include_spirv!(
            "../../../assets/generated/spirv/shader.frag.spv"
        ));

        // Bind group layouts describe data available to the GPU in different shader stages.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                // Binding 0 is a uniform buffer used to hold a transformation matrix for the
                // vertex shader.
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Binding 1 holds a texture that is sampled by texture coordinates to produce the
                // appearance of a particular set of geometry in the fragment shader.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Binding 2 holds a sampling algorithm used to define the behavior when sampling
                // the texture in the fragment shader.
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler {
                        comparison: false,
                        filtering: true,
                    },
                    count: None,
                },
            ],
        });

        let render_format = wgpu::TextureFormat::Bgra8Unorm;
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: render_format,
            width: SIZE_X as u32,
            height: SIZE_Y as u32,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        // A multisampled framebuffer is used for anti-aliasing.
        let multisampled_framebuffer =
            create_multisampled_framebuffer(&device, &sc_desc, MSAA_SAMPLES);

        // The graphics pipeline specifies what behavior to use when rendering to the screen.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: "main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::InputStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float4, 1 => Float2],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: sc_desc.format,
                    color_blend: wgpu::BlendState {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha_blend: wgpu::BlendState {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    write_mask: wgpu::ColorWrite::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::Back,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // We render the background and pointer to the screen as two rectangles with various
        // rotations. These index and vertex buffers describe a single rectangle split into two
        // triangular fragments that is reused for both images.
        let rectangle_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &[
                    Vertex::new(1., 1., 1., 0.),
                    Vertex::new(-1., 1., 0., 0.),
                    Vertex::new(-1., -1., 0., 1.),
                    Vertex::new(1., -1., 1., 1.),
                ]
                .as_bytes(),
                usage: wgpu::BufferUsage::VERTEX,
            });
        let rectangle_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &[0u32, 1, 2, 2, 3, 0].as_bytes(),
            usage: wgpu::BufferUsage::INDEX,
        });

        // Different bind groups for the background and pointer allow them to be rendered with a
        // different appearance. We also save the uniform buffer used to transform the pointer, so
        // that we can give it a different rotation later on. The background doesn't move, so we
        // never need to update its uniform buffer.
        let (background_bind_group, _) = make_bind_group(
            &device,
            &queue,
            &bind_group_layout,
            &sampler,
            BACKGROUND_IMAGE,
            Matrix4::identity(),
        );
        let (pointer_bind_group, pointer_transform_buffer) = make_bind_group(
            &device,
            &queue,
            &bind_group_layout,
            &sampler,
            POINTER_IMAGE,
            *SCALE_MOVE_KNOB_TRANSFORM,
        );

        // Font rendering is conveniently handled by `wgpu_glyph` :)
        let fonts: Vec<wgpu_glyph::ab_glyph::FontArc> =
            vec![wgpu_glyph::ab_glyph::FontArc::try_from_slice(FONT).unwrap()];
        let text_renderer = GlyphBrushBuilder::using_fonts(fonts).build(&device, render_format);

        Self {
            device,
            queue,
            multisampled_framebuffer,
            swap_chain,

            text_renderer,
            local_pool: futures::executor::LocalPool::new(),
            staging_belt: wgpu::util::StagingBelt::new(1024),

            pipeline,
            rectangle_index_buffer,
            rectangle_vertex_buffer,

            background_bind_group,

            pointer_bind_group,
            pointer_transform_buffer,
        }
    }

    /// Render a single frame of the given interface state to the screen.
    pub fn draw_frame(&mut self, state: &super::state::InterfaceState) {
        if let Ok(frame) = self.swap_chain.get_current_frame() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                // Pointer starts at top position in source image. Knob limits are 150 degrees in
                // both directions.
                let data = TransformUniform {
                    transform: (*SCALE_MOVE_KNOB_TRANSFORM
                        * Matrix4::from_angle_z(cgmath::Deg(-state.amplitude_value * 300. + 150.)))
                    .into(),
                };
                self.queue.write_buffer(
                    &self.pointer_transform_buffer,
                    0 as wgpu::BufferAddress,
                    data.as_bytes(),
                );

                {
                    let mut rpass = Self::start_renderpass(
                        &mut encoder,
                        &frame.output.view,
                        &self.multisampled_framebuffer,
                    );
                    rpass.set_pipeline(&self.pipeline);
                    rpass.set_index_buffer(self.rectangle_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    rpass.set_vertex_buffer(0, self.rectangle_vertex_buffer.slice(..));

                    // draw background
                    rpass.set_bind_group(0, &self.background_bind_group, &[]);
                    rpass.draw_indexed(0..6, 0, 0..1);

                    // draw knob pointer
                    rpass.set_bind_group(0, &self.pointer_bind_group, &[]);
                    rpass.draw_indexed(0..6, 0, 0..1);
                }

                let display_val = state.amplitude_value * 2.;

                let int_text = display_val.trunc() as u8;
                let frac_text = (display_val.fract() * 100.).trunc() as u8;
                let text = if frac_text < 10 {
                    format!("{}.0{}", int_text, frac_text)
                } else {
                    format!("{}.{}", int_text, frac_text)
                };

                self.text_renderer.queue(wgpu_glyph::Section {
                    text: vec![wgpu_glyph::Text::default()
                        .with_text(&text)
                        .with_color(FONT_COLOR)
                        .with_font_id(wgpu_glyph::FontId(0))
                        .with_scale(100. * SCALE as f32)],
                    layout: wgpu_glyph::Layout::default_single_line()
                        .h_align(wgpu_glyph::HorizontalAlign::Right)
                        .v_align(wgpu_glyph::VerticalAlign::Center),
                    screen_position: (TEXT_RIGHT_ANCHOR, TEXT_CENTER_Y_ANCHOR),
                    bounds: (SIZE_X as f32, SIZE_Y as f32),
                });
                self.text_renderer
                    .draw_queued(
                        &self.device,
                        &mut self.staging_belt,
                        &mut encoder,
                        &frame.output.view,
                        SIZE_X as u32,
                        SIZE_Y as u32,
                    )
                    .unwrap();
            }
            self.staging_belt.finish();
            self.queue.submit(std::iter::once(encoder.finish()));

            use futures::task::SpawnExt;
            self.local_pool
                .spawner()
                .spawn(self.staging_belt.recall())
                .expect("Recall staging belt");
            self.local_pool.run_until_stalled();
        }
    }

    /// Begin a renderpass for the background and knob pointer. Text will be drawn in a separate
    /// pass by `wgpu_glyph`.
    fn start_renderpass<'a>(
        encoder: &'a mut wgpu::CommandEncoder,
        view: &'a wgpu::TextureView,
        multisampled_framebuffer: &'a wgpu::TextureView,
    ) -> wgpu::RenderPass<'a> {
        let rpass_color_attachment = wgpu::RenderPassColorAttachmentDescriptor {
            attachment: multisampled_framebuffer,
            resolve_target: Some(view),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: true,
            },
        };

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[rpass_color_attachment],
            depth_stencil_attachment: None,
        })
    }
}

/// Different bind groups are used to render sets of geometry in different ways. In this case, the
/// two geometries on the interface (background and knob pointer) are rendered with different
/// textures and 2D positions.
fn make_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    png_image: &[u8],
    initial_transform: Matrix4<f32>,
) -> (wgpu::BindGroup, wgpu::Buffer) {
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: TransformUniform {
            transform: initial_transform.into(),
        }
        .as_bytes(),
        usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
    });

    let decoder = png::Decoder::new(png_image);
    let (info, mut reader) = decoder.read_info().unwrap();
    let mut image_data = vec![0; info.buffer_size()];
    reader.next_frame(&mut image_data).unwrap();

    let texture_extent = wgpu::Extent3d {
        width: info.width,
        height: info.height,
        depth: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
    });

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    queue.write_texture(
        wgpu::TextureCopyView {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        &image_data,
        wgpu::TextureDataLayout {
            offset: 0,
            bytes_per_row: 4 * info.width,
            rows_per_image: 0,
        },
        texture_extent,
    );

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
        label: None,
    });

    (bind_group, uniform_buf)
}

/// Creates a new buffer that is sampled `sample_count` times more densely than the target output
/// surface, producing a more smooth anti-aliased appearance.
fn create_multisampled_framebuffer(
    device: &wgpu::Device,
    sc_desc: &wgpu::SwapChainDescriptor,
    sample_count: u32,
) -> wgpu::TextureView {
    let multisampled_texture_extent = wgpu::Extent3d {
        width: sc_desc.width,
        height: sc_desc.height,
        depth: 1,
    };
    let multisampled_frame_descriptor = &wgpu::TextureDescriptor {
        label: None,
        size: multisampled_texture_extent,
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: sc_desc.format,
        usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
    };

    device
        .create_texture(multisampled_frame_descriptor)
        .create_view(&wgpu::TextureViewDescriptor::default())
}
