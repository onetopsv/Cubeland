struct Uniforms {
    mvp: mat4x4<f32>,
    cam: vec4<f32>,
    // x = bắt đầu sương mù, y = kết thúc, z = bật (1) / tắt (0)
    fog: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) wpos: vec3<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.mvp * vec4<f32>(v.pos, 1.0);
    o.color = v.color;
    o.wpos = v.pos;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let d = distance(i.wpos, u.cam.xyz);
    let f = clamp((d - u.fog.x) / max(u.fog.y - u.fog.x, 0.001), 0.0, 1.0) * u.fog.z;
    let sky = vec3<f32>(0.53, 0.75, 0.95);
    return vec4<f32>(mix(i.color, sky, f), 1.0);
}
