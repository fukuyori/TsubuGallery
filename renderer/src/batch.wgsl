// ピクセル座標 (左上原点) を受け取り NDC へ変換するだけの最小シェーダ。
// 頂点色は CPU 側で線形 RGB へ変換済み。

struct Uniforms {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// 字形のアトラス。図形は左上の白い点を指すので、常にこれを掛けてよい。
@group(0) @binding(1)
var atlas: texture_2d<f32>;
@group(0) @binding(2)
var atlas_sampler: sampler;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(
        pos.x / uniforms.viewport.x * 2.0 - 1.0,
        1.0 - pos.y / uniforms.viewport.y * 2.0,
    );
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = color;
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // アトラスは濃さだけを持つ。色は頂点から来る。
    let coverage = textureSample(atlas, atlas_sampler, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
