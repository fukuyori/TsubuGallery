//! レンダーターゲット管理。

/// マルチサンプル用のカラーテクスチャ。サイズ・フォーマットが変わったときだけ
/// 作り直し、それ以外のフレームでは使い回す。
pub struct MsaaTarget {
    samples: u32,
    current: Option<(wgpu::Texture, wgpu::TextureView, u32, u32, wgpu::TextureFormat)>,
}

impl MsaaTarget {
    pub fn new(samples: u32) -> Self {
        Self { samples, current: None }
    }

    pub fn samples(&self) -> u32 {
        self.samples
    }

    pub fn view(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> &wgpu::TextureView {
        let stale = match &self.current {
            Some((_, _, w, h, f)) => *w != width || *h != height || *f != format,
            None => true,
        };

        if stale {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("tsubu.msaa"),
                size: wgpu::Extent3d {
                    width: width.max(1),
                    height: height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.current = Some((texture, view, width, height, format));
        }

        &self.current.as_ref().expect("just populated").1
    }
}
