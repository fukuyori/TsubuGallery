//! フレームをまたいで残るキャンバス。
//!
//! Processing も p5.js も、`draw()` の中で `background()` を呼ばなければ前の絵が
//! そのまま残る。`background(0, 8)` のような半透明で塗り重ねて残像を作る書き方は
//! つぶやき Processing の定番で、これが無いと大半の作品が違う絵になる。
//!
//! 実装は 2 枚のテクスチャを交互に使う。MSAA の解決先へ描くため「前のフレームを
//! 読みながら同じ場所へ書く」ことができないので、読む側と書く側を分ける。
//! 消さないフレームでは、まず前のフレームを 1 枚の四角形として貼り直してから
//! 図形を描く。

use std::collections::HashMap;

use crate::batch::{BatchRenderer, DEPTH_FORMAT, SAMPLE_COUNT, depth_state};
use crate::draw::{Color, Graphics, ShaderPaint};
use crate::texture::MsaaTarget;

/// テクスチャを描画先いっぱいに貼るシェーダー。頂点バッファを持たず、
/// 画面を覆う三角形 1 枚を頂点番号から組み立てる。
const BLIT_SHADER: &str = r#"
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    var out: Out;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return textureSample(src, samp, in.uv);
}
"#;

/// 貼り付け用のパイプライン。描画先ごとに作り分けて使い回す。
struct Blit {
    shader: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    /// 深度つきのパスと、そうでないパスの両方で使う。別々に要る。
    pipelines: HashMap<(wgpu::TextureFormat, u32, bool), wgpu::RenderPipeline>,
}

impl Blit {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tsubu.blit.shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tsubu.blit.layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tsubu.blit.pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        // 等倍で貼るのが基本だが、ウィンドウサイズ変更の途中では拡大縮小が
        // 挟まるので線形補間にしておく。
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tsubu.blit.sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self { shader, layout, pipeline_layout, sampler, pipelines: HashMap::new() }
    }

    fn bind(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tsubu.blit.bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        })
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        samples: u32,
        depth: bool,
    ) -> &wgpu::RenderPipeline {
        self.pipelines.entry((format, samples, depth)).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("tsubu.blit.pipeline"),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        // 下地を敷き直すだけなので混ぜずに上書きする。
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                // 下地を敷き直すだけなので深さは触らない。ここで書き込むと、
                // このあとの立体がぜんぶ手前に隠されてしまう。
                depth_stencil: depth.then(|| depth_state(false)),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        })
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        samples: u32,
        depth: bool,
        bind: &wgpu::BindGroup,
    ) {
        let pipeline = self.pipeline(device, format, samples, depth);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// つぶやき GLSL を画面いっぱいに塗るパス。
///
/// 作品ごとにパイプラインを 1 本作って持ち続ける。作り直しは高いので、
/// 同じ作品を見ているあいだは使い回す。
struct ShaderStage {
    pipeline_layout: wgpu::PipelineLayout,
    /// `r` `m` `t` `f`。std140 で 24 バイトだが、16 の倍数に切り上げて確保する。
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// 鍵はシェーダーと描画先の組み合わせ。描画先ごとに別のパイプラインが要る。
    pipelines: HashMap<(u64, wgpu::TextureFormat, u32), wgpu::RenderPipeline>,
}

/// uniform ブロックの大きさ。`vec2 r; vec2 m; float t; float f;` を 16 の倍数へ。
const SHADER_UNIFORM_BYTES: u64 = 32;

impl ShaderStage {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tsubu.shader.layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tsubu.shader.pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tsubu.shader.uniforms"),
            size: SHADER_UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tsubu.shader.bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        Self { pipeline_layout, uniforms, bind, pipelines: HashMap::new() }
    }

    /// 作品の WGSL からパイプラインを作る。すでにあれば何もしない。
    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        paint: &ShaderPaint,
        format: wgpu::TextureFormat,
        samples: u32,
    ) {
        let layout = &self.pipeline_layout;
        self.pipelines.entry((paint.key, format, samples)).or_insert_with(|| {
            // 翻訳のときに naga で検証済みなので、ここで転ぶことはない。
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("tsubu.shader.module"),
                source: wgpu::ShaderSource::Wgsl(paint.wgsl.as_ref().into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("tsubu.shader.pipeline"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(crate::shader::VERTEX_ENTRY),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(crate::shader::FRAGMENT_ENTRY),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        // 画面ぜんぶを自分で塗る。混ぜる相手はいない。
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                // 立体は積まないので深さは触らない。
                depth_stencil: Some(depth_state(false)),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                multiview_mask: None,
                cache: None,
            })
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        samples: u32,
        paint: &ShaderPaint,
        resolution: [f32; 2],
    ) {
        // std140 の並び。vec2 が 2 つで 16 バイト、そのあとに float が 2 つ。
        let values: [f32; 8] = [
            resolution[0],
            resolution[1],
            paint.mouse[0],
            paint.mouse[1],
            paint.time,
            paint.frame,
            0.0,
            0.0,
        ];
        queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&values));

        self.ensure_pipeline(device, paint, format, samples);
        let Some(pipeline) = self.pipelines.get(&(paint.key, format, samples)) else { return };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// 交互に使う 2 枚のうちの 1 枚。
struct Face {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

/// フレームをまたいで内容が残る描画先。
pub struct Canvas {
    format: wgpu::TextureFormat,
    msaa: MsaaTarget,
    blit: Blit,
    /// つぶやき GLSL 用。使う作品を開くまで作らない。
    shader: Option<ShaderStage>,
    faces: Vec<Face>,
    /// 立体の前後を決めるためのバッファ。フレームごとに消す。
    depth: Option<wgpu::TextureView>,
    size: (u32, u32),
    /// 直前のフレームが入っている側。
    front: usize,
    /// 一度でも描いたか。最初のフレームは前の絵が無いので必ず塗りつぶす。
    painted: bool,
}

impl Canvas {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            msaa: MsaaTarget::new(SAMPLE_COUNT),
            blit: Blit::new(device),
            shader: None,
            faces: Vec::new(),
            depth: None,
            size: (0, 0),
            front: 0,
            painted: false,
        }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// 蓄積を捨てる。作品を切り替えたときや作り直したときに呼ぶ。
    pub fn reset(&mut self) {
        self.painted = false;
    }

    /// 一度でも描いたか。
    pub fn has_content(&self) -> bool {
        self.painted
    }

    /// 直前のフレームが載っているテクスチャ。まだ何も描いていなければ `None`。
    pub fn front_texture(&self) -> Option<&wgpu::Texture> {
        if !self.painted {
            return None;
        }
        self.faces.get(self.front).map(|f| &f.texture)
    }

    /// 1 フレーム描く。`list.clear` が `None` なら前のフレームの上に重ねる。
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &mut BatchRenderer,
        encoder: &mut wgpu::CommandEncoder,
        g: &Graphics,
        width: u32,
        height: u32,
    ) {
        let list = g.draw_list();
        batch.upload_atlas(queue, &g.font);

        let width = width.max(1);
        let height = height.max(1);
        self.ensure(device, width, height);

        // 消す色が指定されていなければ前のフレームを残す。ただし 1 枚目だけは
        // 残すものが無いので、方言ごとの下地で始める。GLSL 作品は全画素を
        // 自分で塗るため、前のフレームを敷き直す必要が無い。
        let clear = match &list.shader {
            Some(_) => Some(Color::BLACK),
            None => list.clear.or((!self.painted).then_some(g.default_background())),
        };

        if list.shader.is_none() {
            batch.prepare(
                device,
                queue,
                list,
                [width as f32, height as f32],
                self.format,
                SAMPLE_COUNT,
            );
        }

        let back = 1 - self.front;
        let msaa_view = self.msaa.view(device, width, height, self.format).clone();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tsubu.canvas.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&self.faces[back].view),
                    ops: wgpu::Operations {
                        // 非 sRGB フォーマットなので sRGB の値をそのまま書く。
                        load: wgpu::LoadOp::Clear(match clear {
                            Some(c) => wgpu::Color {
                                r: c.r as f64,
                                g: c.g as f64,
                                b: c.b as f64,
                                a: c.a as f64,
                            },
                            None => wgpu::Color::TRANSPARENT,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                // 前の絵は色だけ残す。前後の関係はフレームごとに決め直す。
                depth_stencil_attachment: self.depth.as_ref().map(|view| {
                    wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            match &list.shader {
                Some(paint) => {
                    let stage = self.shader.get_or_insert_with(|| ShaderStage::new(device));
                    stage.draw(
                        device,
                        queue,
                        &mut pass,
                        self.format,
                        SAMPLE_COUNT,
                        paint,
                        [width as f32, height as f32],
                    );
                }
                None => {
                    if clear.is_none() {
                        self.blit.draw(
                            device,
                            &mut pass,
                            self.format,
                            SAMPLE_COUNT,
                            true,
                            &self.faces[self.front].bind,
                        );
                    }
                    batch.render(&mut pass);
                }
            }
        }

        self.front = back;
        self.painted = true;
    }

    /// 蓄積した絵を、開いているパスへ貼る。
    pub fn present(
        &mut self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        samples: u32,
    ) {
        if !self.painted {
            return;
        }
        let bind = &self.faces[self.front].bind;
        self.blit.draw(device, pass, format, samples, false, bind);
    }

    fn ensure(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.size == (width, height) && self.faces.len() == 2 {
            return;
        }

        self.faces = (0..2)
            .map(|_| {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("tsubu.canvas.face"),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let bind = self.blit.bind(device, &view);
                Face { texture, view, bind }
            })
            .collect();

        // 深さは色と同じ枚数だけ重ねる。MSAA の色に合わせないと使えない。
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tsubu.canvas.depth"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth = Some(depth.create_view(&wgpu::TextureViewDescriptor::default()));

        self.size = (width, height);
        self.front = 0;
        // 大きさが変わったら前の絵は使えない。
        self.painted = false;
    }
}
