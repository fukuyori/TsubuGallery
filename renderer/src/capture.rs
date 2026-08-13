//! オフスクリーン描画とフレーム取得 (設計書 §7 / Prototype C)。
//!
//! Viewer と同じ [`BatchRenderer`] を使うため、サムネイルは実行結果と同じ見た目に
//! なる。ウィンドウのサイズや可視状態には依存しない。

use crate::batch::BatchRenderer;
use crate::canvas::Canvas;
use crate::draw::Graphics;

/// GPU から読み出した RGBA8 (sRGB エンコード済み) 画像。
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("GPU バッファのマップに失敗しました: {0}")]
    Map(#[from] wgpu::BufferAsyncError),
    #[error("GPU の待機に失敗しました: {0}")]
    Poll(String),
    #[error("読み出せる絵がありません")]
    Empty,
}

/// オフスクリーンのキャンバスと読み戻し用バッファ。同じサイズで繰り返し使う。
///
/// 残像を使う作品のサムネイルは、狙ったフレームまで実際に積み上げないと本物と
/// 同じ絵にならない。そのため 1 枚ずつ [`Capturer::draw`] へ流し込み、最後に
/// [`Capturer::read`] で取り出す。
pub struct Capturer {
    canvas: Option<Canvas>,
    format: wgpu::TextureFormat,
    readback: Option<(wgpu::Buffer, u32, u32)>,
}

impl Default for Capturer {
    fn default() -> Self {
        Self::new()
    }
}

impl Capturer {
    pub fn new() -> Self {
        Self { canvas: None, format: wgpu::TextureFormat::Rgba8Unorm, readback: None }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// 蓄積を捨てて、新しい作品の 1 枚目から積み直す。
    pub fn begin(&mut self) {
        if let Some(canvas) = &mut self.canvas {
            canvas.reset();
        }
    }

    /// 1 フレーム描き足す。
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &mut BatchRenderer,
        g: &Graphics,
        width: u32,
        height: u32,
    ) {
        let width = width.max(1);
        let height = height.max(1);
        let format = self.format;
        let canvas = self.canvas.get_or_insert_with(|| Canvas::new(device, format));

        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tsubu.capture") });
        canvas.render(device, queue, batch, &mut encoder, g, width, height);
        queue.submit(Some(encoder.finish()));
    }

    /// 積み上げた絵を CPU へ読み戻す。
    pub fn read(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<CapturedImage, CaptureError> {
        let width = width.max(1);
        let height = height.max(1);
        let bytes_per_row = padded_bytes_per_row(width);

        self.ensure_readback(device, width, height);
        let (buffer, _, _) = self.readback.as_ref().expect("readback ensured");
        let texture = self
            .canvas
            .as_ref()
            .and_then(|c| c.front_texture())
            .ok_or(CaptureError::Empty)?;

        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tsubu.readback") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit(Some(encoder.finish()));

        let (buffer, _, _) = self.readback.as_ref().expect("readback ensured");
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| CaptureError::Poll(e.to_string()))?;
        rx.recv().expect("map callback dropped")?;

        let rgba = {
            let view = slice.get_mapped_range().map_err(|e| CaptureError::Poll(e.to_string()))?;
            let mut out = Vec::with_capacity((width * height * 4) as usize);
            for row in 0..height as usize {
                let start = row * bytes_per_row as usize;
                out.extend_from_slice(&view[start..start + (width * 4) as usize]);
            }
            out
        };
        buffer.unmap();

        Ok(CapturedImage { width, height, rgba })
    }

    /// 1 枚だけ描いて読み戻す近道。
    pub fn capture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &mut BatchRenderer,
        g: &Graphics,
        width: u32,
        height: u32,
    ) -> Result<CapturedImage, CaptureError> {
        self.begin();
        self.draw(device, queue, batch, g, width, height);
        self.read(device, queue, width, height)
    }

    fn ensure_readback(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if matches!(self.readback, Some((_, w, h)) if w == width && h == height) {
            return;
        }
        let size = padded_bytes_per_row(width) as u64 * height as u64;
        self.readback = Some((
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tsubu.capture.readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            width,
            height,
        ));
    }
}

fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded.div_ceil(align) * align
}

#[cfg(test)]
mod tests {
    use super::padded_bytes_per_row;

    #[test]
    fn row_padding_is_aligned() {
        assert_eq!(padded_bytes_per_row(1), 256);
        assert_eq!(padded_bytes_per_row(64), 256);
        assert_eq!(padded_bytes_per_row(65), 512);
        assert_eq!(padded_bytes_per_row(320), 1280);
    }
}
