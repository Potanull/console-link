struct Params {
    basic: vec4<f32>,
    tone: vec4<f32>,
    output_flags: vec4<f32>,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var v: Vertex;
    v.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    v.uv = uv;
    return v;
}
fn decode(rgb: vec3<f32>) -> vec3<f32> {
    return select(pow((rgb + 0.055) / 1.055, vec3<f32>(2.4)), rgb / 12.92, rgb <= vec3<f32>(0.04045));
}
fn encode(rgb: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(rgb, vec3<f32>(1.0 / 2.4)) - 0.055, rgb * 12.92, rgb <= vec3<f32>(0.0031308));
}
@fragment fn fs_main(v: Vertex) -> @location(0) vec4<f32> {
    let pixel = textureSample(source, source_sampler, v.uv);
    var rgb = pixel.rgb;
    if params.tone.w > 0.5 {
        rgb = decode(rgb) * exp2(params.basic.x) * vec3<f32>(exp2(params.basic.w * 0.12), 1.0, exp2(-params.basic.w * 0.12));
        let y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let sw = pow(1.0 - clamp(y / 0.5, 0.0, 1.0), 2.0);
        let hw = pow(clamp((y - 0.25) / 0.75, 0.0, 1.0), 2.0);
        rgb *= exp2(params.tone.x * 2.0 * sw + params.tone.y * 1.5 * hw);
        let h = 0.6 * params.tone.z;
        rgb = rgb * (1.0 + h) / (vec3<f32>(1.0) + h * rgb);
        let current_y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        rgb = max(vec3<f32>(current_y) + (rgb - current_y) * params.basic.z, vec3<f32>(0.0));
        rgb = clamp((encode(rgb) - 0.5) * params.basic.y + 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if params.output_flags.x > 0.5 { rgb = decode(rgb); }
    return vec4<f32>(rgb, pixel.a);
}
