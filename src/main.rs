//! Cubeland client: wgpu + winit, bộ nút cảm ứng kiểu launcher (iOS).
//! Toàn bộ logic nút/HUD nằm trong `cubeland::controls` (có test), file này chỉ nối vào GPU.

use std::{collections::HashMap, sync::Arc, time::Instant};

use bytemuck::{Pod, Zeroable};
use cubeland::{
    controls::{Controls, Phase},
    math, mesh,
    player::Player,
    world::{Block, World, CS},
};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::{TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const RADIUS: i32 = 5; // bán kính vẽ (chunk)
const LOOK_SENS: f32 = 0.004; // rad / pixel
const GEN_BUDGET: usize = 2; // số chunk sinh mỗi frame
const MESH_BUDGET: usize = 2; // số mesh dựng mỗi frame
const UI_MAX_VERTS: usize = 40_000; // bộ đệm UI: nửa đầu cho nền nút, nửa sau cho chữ/ký hiệu
const UI_HALF: usize = UI_MAX_VERTS / 2;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    cam: [f32; 4],
    fog: [f32; 4],
}

struct GpuMesh {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    n: u32,
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    depth: wgpu::TextureView,
    ubuf_world: wgpu::Buffer,
    bg_world: wgpu::BindGroup,
    bg_ui: wgpu::BindGroup,
    bg_ui_alpha: wgpu::BindGroup,
    ui_vb: wgpu::Buffer,
    body_vb: wgpu::Buffer,
    body_ib: wgpu::Buffer,

    world: World,
    player: Player,
    meshes: HashMap<(i32, i32), Option<GpuMesh>>,
    offsets: Vec<(i32, i32)>,

    controls: Controls,
    last: Instant,
    fps: f32,
}

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl State {
    async fn new(window: Arc<Window>) -> State {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("create_surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("không tìm thấy GPU adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("request_device");

        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface config");
        // Màu trong game là giá trị "thô", nên chọn format không-sRGB để khỏi bị bạc màu.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = f;
        }
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Ba bộ uniform: thế giới, UI đặc (alpha 1), UI trong suốt (alpha 0.55).
        let ui_uniforms = |alpha: f32| Uniforms {
            mvp: math::identity(),
            cam: [0.0; 4],
            fog: [0.0, 1.0, 0.0, alpha],
        };
        let ubuf_world = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ubuf_world"),
            contents: bytemuck::bytes_of(&ui_uniforms(1.0)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ubuf_ui = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ubuf_ui"),
            contents: bytemuck::bytes_of(&ui_uniforms(1.0)),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let ubuf_ui_alpha = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ubuf_ui_alpha"),
            contents: bytemuck::bytes_of(&ui_uniforms(0.55)),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let make_bg = |buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
            })
        };
        let bg_world = make_bg(&ubuf_world);
        let bg_ui = make_bg(&ubuf_ui);
        let bg_ui_alpha = make_bg(&ubuf_ui_alpha);

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let vattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let make_pipeline = |blend: Option<wgpu::BlendState>, depth_write: bool, cmp: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 24,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vattrs,
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                // Không cull: tránh lỗi thứ tự đỉnh làm mất mặt.
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: depth_write,
                    depth_compare: cmp,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let pipeline = make_pipeline(None, true, wgpu::CompareFunction::Less);
        // UI: pha trộn alpha, luôn vẽ đè lên thế giới, không ghi độ sâu.
        let ui_pipeline = make_pipeline(Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Always);

        let ui_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui_vb"),
            size: (UI_MAX_VERTS * 24) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let body_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("body_vb"),
            size: 2048,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let body_ib = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("body_ib"),
            size: 512,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth = make_depth(&device, config.width, config.height);

        // Sinh vùng quanh điểm xuất hiện rồi đặt người chơi lên mặt đất.
        let mut world = World::new(1337);
        for cx in -2..=2 {
            for cz in -2..=2 {
                world.ensure_chunk(cx, cz);
            }
        }
        let top = world.column_top(8, 8);
        let player = Player::new([8.5, (top + 1) as f32, 8.5]);

        let mut offsets = Vec::new();
        for dx in -(RADIUS + 1)..=(RADIUS + 1) {
            for dz in -(RADIUS + 1)..=(RADIUS + 1) {
                offsets.push((dx, dz));
            }
        }
        offsets.sort_by_key(|(dx, dz)| dx * dx + dz * dz);

        let controls = Controls::new(config.width as f32, config.height as f32, window.scale_factor() as f32);

        State {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            ui_pipeline,
            depth,
            ubuf_world,
            bg_world,
            bg_ui,
            bg_ui_alpha,
            ui_vb,
            body_vb,
            body_ib,
            world,
            player,
            meshes: HashMap::new(),
            offsets,
            controls,
            last: Instant::now(),
            fps: 60.0,
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.depth = make_depth(&self.device, w, h);
        self.controls.resize(w as f32, h as f32, self.window.scale_factor() as f32);
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    // ---------- input ----------

    fn touch(&mut self, t: winit::event::Touch) {
        let phase = match t.phase {
            TouchPhase::Started => Phase::Start,
            TouchPhase::Moved => Phase::Move,
            TouchPhase::Ended | TouchPhase::Cancelled => Phase::End,
        };
        let p = [t.location.x as f32, t.location.y as f32];
        if let Some(d) = self.controls.touch(t.id, phase, p) {
            self.player.yaw += d[0] * LOOK_SENS;
            self.player.pitch = (self.player.pitch - d[1] * LOOK_SENS).clamp(-1.5, 1.5);
        }
    }

    // ---------- camera ----------

    /// Lùi camera dọc theo hướng d tối đa 4 ô, dừng trước khi chạm block.
    fn back_off(&self, eye: [f32; 3], d: [f32; 3]) -> [f32; 3] {
        let mut best = 0.0f32;
        let mut t = 0.2f32;
        while t <= 4.0 {
            let p = [eye[0] + d[0] * t, eye[1] + d[1] * t, eye[2] + d[2] * t];
            if self.world.get_block(p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32).solid() {
                break;
            }
            best = t;
            t += 0.2;
        }
        [eye[0] + d[0] * best, eye[1] + d[1] * best, eye[2] + d[2] * best]
    }

    /// (vị trí camera, hướng nhìn) theo chế độ 1st / sau lưng / phía trước.
    fn camera(&self) -> ([f32; 3], [f32; 3]) {
        let eye = self.player.eye();
        let dir = self.player.dir();
        let back = [-dir[0], -dir[1], -dir[2]];
        match self.controls.third {
            0 => (eye, dir),
            1 => (self.back_off(eye, back), dir),
            _ => (self.back_off(eye, dir), back),
        }
    }

    // ---------- update ----------

    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;
        self.fps = self.fps * 0.9 + (1.0 / dt.max(1e-4)) * 0.1;

        self.controls.tick(dt);
        if self.controls.paused {
            return;
        }

        self.player.sneaking = self.controls.sneaking;
        let wish = self.controls.wish();
        let jump = self.controls.jump();
        self.player.update(&self.world, dt, wish, jump);

        let eye = self.player.eye();
        let dir = self.player.dir();
        if std::mem::take(&mut self.controls.break_req) {
            if let Some((hit, _)) = self.world.raycast(eye, dir, 6.0) {
                if hit[1] > 0 {
                    self.world.set_block(hit[0], hit[1], hit[2], Block::Air);
                }
            }
        }
        if std::mem::take(&mut self.controls.place_req) {
            if let Some((hit, prev)) = self.world.raycast(eye, dir, 6.0) {
                if prev != hit && !self.player.intersects_block(prev) {
                    let b = self.controls.selected_block();
                    self.world.set_block(prev[0], prev[1], prev[2], b);
                }
            }
        }

        // Stream chunk: gần trước, xa sau.
        let pcx = (self.player.pos[0].floor() as i32).div_euclid(CS);
        let pcz = (self.player.pos[2].floor() as i32).div_euclid(CS);

        let mut gen_left = GEN_BUDGET;
        for &(dx, dz) in &self.offsets {
            if gen_left == 0 {
                break;
            }
            if !self.world.has_chunk(pcx + dx, pcz + dz) {
                self.world.ensure_chunk(pcx + dx, pcz + dz);
                gen_left -= 1;
            }
        }

        let mut mesh_left = MESH_BUDGET;
        for i in 0..self.offsets.len() {
            if mesh_left == 0 {
                break;
            }
            let (dx, dz) = self.offsets[i];
            if dx.abs() > RADIUS || dz.abs() > RADIUS {
                continue;
            }
            let key = (pcx + dx, pcz + dz);
            let need = !self.meshes.contains_key(&key) || self.world.dirty.contains(&key);
            if !need {
                continue;
            }
            let ready = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .all(|(ax, az)| self.world.has_chunk(key.0 + ax, key.1 + az));
            if !ready {
                continue;
            }
            let (v, idx) = mesh::build_chunk_mesh(&self.world, key.0, key.1);
            self.world.dirty.remove(&key);
            let gpu = if idx.is_empty() {
                None
            } else {
                let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&v),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&idx),
                    usage: wgpu::BufferUsages::INDEX,
                });
                Some(GpuMesh { vb, ib, n: idx.len() as u32 })
            };
            self.meshes.insert(key, gpu);
            mesh_left -= 1;
        }

        // Bỏ mesh ở xa để tiết kiệm RAM (dữ liệu block vẫn giữ nên không mất công trình).
        self.meshes
            .retain(|k, _| (k.0 - pcx).abs() <= RADIUS + 2 && (k.1 - pcz).abs() <= RADIUS + 2);
    }

    // ---------- render ----------

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let (cam, look) = self.camera();
        let fov = if self.controls.zoom() { 25.0f32 } else { 70.0f32 };
        let proj = math::perspective(fov.to_radians(), aspect, 0.1, 400.0);
        let viewm = math::look_to(cam, look);
        let far = (RADIUS * CS) as f32;
        let u = Uniforms {
            mvp: math::mul(&proj, &viewm),
            cam: [cam[0], cam[1], cam[2], 0.0],
            fog: [far * 0.55, far * 0.95, 1.0, 1.0],
        };
        self.queue.write_buffer(&self.ubuf_world, 0, bytemuck::bytes_of(&u));

        // Thân người chơi (chỉ khi nhìn góc thứ ba)
        let mut body_idx_count = 0u32;
        if self.controls.third != 0 {
            let p = self.player.pos;
            let (mut v, mut i) = (Vec::new(), Vec::new());
            mesh::push_box(&mut v, &mut i, [p[0] - 0.3, p[1], p[2] - 0.2], [p[0] + 0.3, p[1] + 1.35, p[2] + 0.2], [0.2, 0.3, 0.7]);
            mesh::push_box(&mut v, &mut i, [p[0] - 0.25, p[1] + 1.35, p[2] - 0.25], [p[0] + 0.25, p[1] + 1.85, p[2] + 0.25], [0.8, 0.6, 0.45]);
            self.queue.write_buffer(&self.body_vb, 0, bytemuck::cast_slice(&v));
            self.queue.write_buffer(&self.body_ib, 0, bytemuck::cast_slice(&i));
            body_idx_count = i.len() as u32;
        }

        // HUD
        let yaw_deg = self.player.yaw.to_degrees().rem_euclid(360.0);
        let lines = vec![
            format!("XYZ: {:.1} {:.1} {:.1}", self.player.pos[0], self.player.pos[1], self.player.pos[2]),
            format!("FPS: {:.0}", self.fps),
            format!("CHUNKS: {}", self.meshes.len()),
            format!("FACING: {:.0}", yaw_deg),
        ];
        let (mut bg, mut fg) = self.controls.hud(&lines);
        let cap = (UI_HALF / 6) * 36; // số float tối đa mỗi nửa bộ đệm, chia hết cho 36
        bg.truncate(cap);
        fg.truncate(cap);
        if !bg.is_empty() {
            self.queue.write_buffer(&self.ui_vb, 0, bytemuck::cast_slice(&bg));
        }
        if !fg.is_empty() {
            self.queue.write_buffer(&self.ui_vb, (UI_HALF * 24) as u64, bytemuck::cast_slice(&fg));
        }
        let (bg_n, fg_n) = ((bg.len() / 6) as u32, (fg.len() / 6) as u32);

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.53, g: 0.75, b: 0.95, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bg_world, &[]);
            for m in self.meshes.values().flatten() {
                pass.set_vertex_buffer(0, m.vb.slice(..));
                pass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.n, 0, 0..1);
            }
            if body_idx_count > 0 {
                pass.set_vertex_buffer(0, self.body_vb.slice(..));
                pass.set_index_buffer(self.body_ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..body_idx_count, 0, 0..1);
            }

            // UI: nền nút trong suốt trước, chữ và ký hiệu đặc sau.
            pass.set_pipeline(&self.ui_pipeline);
            if bg_n > 0 {
                pass.set_bind_group(0, &self.bg_ui_alpha, &[]);
                pass.set_vertex_buffer(0, self.ui_vb.slice(0..(bg_n as u64 * 24)));
                pass.draw(0..bg_n, 0..1);
            }
            if fg_n > 0 {
                pass.set_bind_group(0, &self.bg_ui, &[]);
                let start = (UI_HALF * 24) as u64;
                pass.set_vertex_buffer(0, self.ui_vb.slice(start..start + fg_n as u64 * 24));
                pass.draw(0..fg_n, 0..1);
            }
        }
        self.queue.submit(Some(enc.finish()));
        frame.present();
        Ok(())
    }
}

#[derive(Default)]
struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(Window::default_attributes().with_title("Cubeland"))
                    .expect("create_window"),
            );
            self.state = Some(pollster::block_on(State::new(window)));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => s.resize(size.width, size.height),
            WindowEvent::Touch(t) => s.touch(t),
            WindowEvent::RedrawRequested => {
                s.update();
                match s.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => s.reconfigure(),
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(_) => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(s) = &self.state {
            s.window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run_app");
}
