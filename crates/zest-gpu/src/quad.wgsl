struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

@group(0) @binding(0) var image_texture: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VertexOut {
    var result: VertexOut;
    result.position = vec4<f32>(position, 0.0, 1.0);
    result.color = color;
    result.uv = uv;
    return result;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(image_texture, image_sampler, input.uv);
    return vec4<f32>(texel.rgb * texel.a * input.color.rgb, texel.a * input.color.a);
}
