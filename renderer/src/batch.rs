//! [`DrawList`] を GPU へ流し込むバッチレンダラ。
//!
//! 図形はすべて三角形へ展開済みなので、1 フレームは原則 1 ドローコールで済む。
//! パイプラインは (フォーマット, サンプル数) ごとに遅延生成し、ウィンドウ描画と
//! オフスクリーンのサムネイル生成で同じインスタンスを共有する。

use std::collections::HashMap;

use wgpu::util::DeviceExt as _;

use crate::draw::{Batch, BlendMode, DrawList, Vertex};

/// 全レンダーターゲットで共通のサンプル数。
pub const SAMPLE_COUNT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    viewport: [f32; 2],
    _pad: [f32; 2],
}

/// パイプラインを分ける条件。合成方法ごとに別々に要る。
type PipelineKey = (wgpu::TextureFormat, u32, BlendMode);

pub struct BatchRenderer {
    shader: wgpu::ShaderModule,
    atlas: wgpu::Texture,
    /// 送り済みのアトラスの版。変わったときだけ送り直す。
    atlas_version: u64,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    vertex_capacity: u64,
    index_capacity: u64,
    /// 直近の [`BatchRenderer::prepare`] が用意した描画内容。
    pending: Option<Pending>,
}

struct Pending {
    format: wgpu::TextureFormat,
    samples: u32,
    /// 描く区間。合成方法ごとに分かれている。
    batches: Vec<Batch>,
}

impl BatchRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tsubu.batch.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("batch.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tsubu.batch.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tsubu.batch.uniforms"),
            contents: bytemuck::bytes_of(&Uniforms { viewport: [1.0, 1.0], _pad: [0.0; 2] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 字形のアトラス。中身は濃さだけなので 1 チャンネルで足りる。
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tsubu.batch.atlas"),
            size: wgpu::Extent3d {
                width: crate::font::ATLAS_SIZE as u32,
                height: crate::font::ATLAS_SIZE as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tsubu.batch.atlas_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tsubu.batch.bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tsubu.batch.layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        const INITIAL_VERTICES: u64 = 4096;
        const INITIAL_INDICES: u64 = 8192;

        Self {
            shader,
            pipeline_layout,
            pipelines: HashMap::new(),
            bind_group,
            uniform_buffer,
            atlas,
            atlas_version: 0,
            vertex_buffer: alloc_vertex_buffer(device, INITIAL_VERTICES),
            index_buffer: alloc_index_buffer(device, INITIAL_INDICES),
            vertex_capacity: INITIAL_VERTICES,
            index_capacity: INITIAL_INDICES,
            pending: None,
        }
    }

    /// 字形のアトラスを GPU へ送る。中身が変わったときだけ働く。
    pub fn upload_atlas(&mut self, queue: &wgpu::Queue, atlas: &crate::font::FontAtlas) {
        if self.atlas_version == atlas.version() {
            return;
        }
        let size = crate::font::ATLAS_SIZE as u32;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.pixels(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
        self.atlas_version = atlas.version();
    }

    /// 描画内容を GPU バッファへ転送する。レンダーパス開始前に呼ぶ。
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DrawList,
        viewport: [f32; 2],
        format: wgpu::TextureFormat,
        samples: u32,
    ) {
        for batch in &list.batches {
            self.ensure_pipeline(device, (format, samples, batch.blend));
        }

        if list.indices.is_empty() {
            self.pending = Some(Pending { format, samples, batches: Vec::new() });
            return;
        }

        if list.vertices.len() as u64 > self.vertex_capacity {
            self.vertex_capacity = (list.vertices.len() as u64).next_power_of_two();
            self.vertex_buffer = alloc_vertex_buffer(device, self.vertex_capacity);
        }
        if list.indices.len() as u64 > self.index_capacity {
            self.index_capacity = (list.indices.len() as u64).next_power_of_two();
            self.index_buffer = alloc_index_buffer(device, self.index_capacity);
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&list.vertices));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&list.indices));
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&Uniforms { viewport, _pad: [0.0; 2] }),
        );

        self.pending = Some(Pending {
            format,
            samples,
            batches: list.batches.clone(),
        });
    }

    /// [`BatchRenderer::prepare`] 済みの内容を描画する。
    ///
    /// 合成方法が変わるところでパイプラインを差し替える。ふつうの作品は
    /// 区間が 1 つなので、実質 1 ドローコールのまま。
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(pending) = self.pending.as_ref() else { return };
        if pending.batches.is_empty() {
            return;
        }

        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        let mut current: Option<BlendMode> = None;
        for batch in &pending.batches {
            if current != Some(batch.blend) {
                let key = (pending.format, pending.samples, batch.blend);
                let Some(pipeline) = self.pipelines.get(&key) else { continue };
                pass.set_pipeline(pipeline);
                current = Some(batch.blend);
            }
            pass.draw_indexed(batch.start..batch.end, 0, 0..1);
        }
    }

    fn ensure_pipeline(&mut self, device: &wgpu::Device, key: PipelineKey) {
        if self.pipelines.contains_key(&key) {
            return;
        }
        let (format, samples, blend) = key;
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tsubu.batch.pipeline"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x4,
                        2 => Float32x2,
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend_state(blend)),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // つぶやき系のコードは巻き方向を気にしないので、裏面も描く。
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, pipeline);
    }
}

/// 合成方法を GPU の設定へ移す。
///
/// 色はストレートアルファのまま持っているので、`src_alpha` を掛けるのは GPU 側。
fn blend_state(blend: BlendMode) -> wgpu::BlendState {
    use wgpu::{BlendComponent, BlendFactor, BlendOperation, BlendState};

    let alpha = BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    };

    match blend {
        BlendMode::Blend => BlendState::ALPHA_BLENDING,
        // 加算。重なるほど明るくなる。
        BlendMode::Add => BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            alpha,
        },
        // 乗算。重なるほど暗くなる。
        BlendMode::Multiply => BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::Dst,
                dst_factor: BlendFactor::Zero,
                operation: BlendOperation::Add,
            },
            alpha,
        },
        // スクリーン。加算より穏やかに明るくなる。
        BlendMode::Screen => BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::OneMinusSrc,
                operation: BlendOperation::Add,
            },
            alpha,
        },
    }
}

fn alloc_vertex_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tsubu.batch.vertices"),
        size: capacity * std::mem::size_of::<Vertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn alloc_index_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tsubu.batch.indices"),
        size: capacity * std::mem::size_of::<u32>() as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
