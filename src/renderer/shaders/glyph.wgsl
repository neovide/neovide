// Vertex shader

struct CameraUniform {
    view_proj: mat4x4<f32>,
    row_height: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
};
struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) width: f32,
    @location(3) color: vec4<f32>,
    @location(4) uv: vec4<f32>,
    @location(5) texture: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;
    let box = vec2<f32>(model.position.x * instance.width, model.position.y * camera.row_height) + instance.position;

    let position = vec4<f32>(box, 0.0, 1.0);
    out.clip_position = camera.view_proj * position;
    //out.color = instance.color;
    //out.clip_position = camera.view_proj * vec4<f32>(model.position * 500.0, 0.0, 1.0);
    out.color = instance.color;
    return out;
}

// Fragment shader

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
 
