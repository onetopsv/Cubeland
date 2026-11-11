//! Cubeland client: wgpu + winit, điều khiển cảm ứng (iOS).
//!
//! Điều khiển:
//!  - Nửa trái màn hình: kéo = joystick di chuyển, chạm nhanh = nhảy
//!  - Nửa phải màn hình: kéo = xoay camera, chạm nhanh = phá block
//!  - Chạm nhanh góc dưới-phải = đặt block
//!  - Chạm nhanh góc trên-phải = đổi loại block

use std::{collections::HashMap, sync::Arc, time::Instant};

use bytemuck::{Pod, Zeroable};
use cubeland::{
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
const UI_MAX_QUADS: usize = 64;

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

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Move,
    Look,
}

struct TouchInfo {
    start: [f32; 2],
    cur: [f32; 2],
    t0: Instant,
    moved: f32,
    role: Role,
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    depth: wgpu::TextureView,
    ubuf_world: wgpu::Buffer,
    bg_world: wgpu::BindGroup,
    bg_ui: wgpu::BindGroup,
    ui_vb: wgpu::Buffer,

    world: World,
    player: Player,
    meshes: HashMap<(i32, i32), Option<GpuMesh>>,
    offsets: Vec<(i32, i32)>,

    touches: HashMap<u64, TouchInfo>,
    jump_req: bool,
    break_req: bool,
    place_req: bool,
    selected: usize,
    last: Instant,
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

        let zero = Uniforms { mvp: math::identity(), cam: [0.0; 4], fog: [0.0, 1.0, 0.0, 0.0] };
        let ubuf_world = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ubuf_world"),
            contents: bytemuck::bytes_of(&zero),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ubuf_ui = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ubuf_ui"),
            contents: bytemuck::bytes_of(&zero), // identity, sương mù tắt
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

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let vattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
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
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            // Không cull: tránh lỗi thứ tự đỉnh làm mất mặt.
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        let ui_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui_vb"),
            size: (UI_MAX_QUADS * 6 * 24) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
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

        State {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            depth,
            ubuf_world,
            bg_world,
            bg_ui,
            ui_vb,
            world,
            player,
            meshes: HashMap::new(),
            offsets,
            touches: HashMap::new(),
            jump_req: false,
            break_req: false,
            place_req: false,
            selected: 0,
            last: Instant::now(),
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
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    // ---------- input ----------

    fn touch(&mut self, t: winit::event::Touch) {
        let p = [t.location.x as f32, t.location.y as f32];
        let w = self.config.width as f32;
        match t.phase {
            TouchPhase::Started => {
                let role = if p[0] < w * 0.5 { Role::Move } else { Role::Look };
                self.touches
                    .insert(t.id, TouchInfo { start: p, cur: p, t0: Instant::now(), moved: 0.0, role });
            }
            TouchPhase::Moved => {
                if let Some(ti) = self.touches.get_mut(&t.id) {
                    let dx = p[0] - ti.cur[0];
                    let dy = p[1] - ti.cur[1];
                    ti.cur = p;
                    let ds = ((p[0] - ti.start[0]).powi(2) + (p[1] - ti.start[1]).powi(2)).sqrt();
                    ti.moved = ti.moved.max(ds);
                    if ti.role == Role::Look {
                        self.player.yaw += dx * LOOK_SENS;
                        self.player.pitch = (self.player.pitch - dy * LOOK_SENS).clamp(-1.5, 1.5);
                    }
                }
            }
            TouchPhase::Ended => {
                if let Some(ti) = self.touches.remove(&t.id) {
                    let scale = self.window.scale_factor() as f32;
                    if ti.moved < 18.0 * scale && ti.t0.elapsed().as_secs_f32() < 0.3 {
                        self.tap(ti.start, ti.role);
                    }
                }
            }
            TouchPhase::Cancelled => {
                self.touches.remove(&t.id);
            }
        }
    }

    fn tap(&mut self, p: [f32; 2], role: Role) {
        let w = self.config.width as f32;
        let h = self.config.height as f32;
        match role {
            Role::Move => self.jump_req = true,
            Role::Look => {
                if p[0] > w * 0.85 && p[1] < h * 0.2 {
                    self.selected = (self.selected + 1) % Block::PLACEABLE.len();
                } else if p[0] > w * 0.8 && p[1] > h * 0.7 {
                    self.place_req = true;
                } else {
                    self.break_req = true;
                }
            }
        }
    }

    fn joystick(&self) -> [f32; 2] {
        let r = 70.0 * self.window.scale_factor() as f32;
        for ti in self.touches.values() {
            if ti.role == Role::Move {
                let x = (ti.cur[0] - ti.start[0]) / r;
                let y = -(ti.cur[1] - ti.start[1]) / r;
                let len = (x * x + y * y).sqrt();
                return if len > 1.0 { [x / len, y / len] } else { [x, y] };
            }
        }
        [0.0, 0.0]
    }

    // ---------- update ----------

    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;

        let wish = self.joystick();
        let jump = std::mem::take(&mut self.jump_req);
        self.player.update(&self.world, dt, wish, jump);

        let eye = self.player.eye();
        let dir = self.player.dir();
        if std::mem::take(&mut self.break_req) {
            if let Some((hit, _)) = self.world.raycast(eye, dir, 6.0) {
                if hit[1] > 0 {
                    self.world.set_block(hit[0], hit[1], hit[2], Block::Air);
                }
            }
        }
        if std::mem::take(&mut self.place_req) {
            if let Some((hit, prev)) = self.world.raycast(eye, dir, 6.0) {
                if prev != hit && !self.player.intersects_block(prev) {
                    let b = Block::PLACEABLE[self.selected];
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

    // ---------- UI overlay ----------

    fn build_ui(&self) -> Vec<f32> {
        let w = self.config.width as f32;
        let h = self.config.height as f32;
        let s = self.window.scale_factor() as f32;
        let mut v: Vec<f32> = Vec::new();
        let mut quad = |x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 3]| {
            let (nx0, nx1) = (x0 / w * 2.0 - 1.0, x1 / w * 2.0 - 1.0);
            let (ny0, ny1) = (1.0 - y0 / h * 2.0, 1.0 - y1 / h * 2.0);
            for (x, y) in [(nx0, ny0), (nx1, ny0), (nx1, ny1), (nx0, ny0), (nx1, ny1), (nx0, ny1)] {
                v.extend_from_slice(&[x, y, 0.0, c[0], c[1], c[2]]);
            }
        };
        let white = [1.0, 1.0, 1.0];

        // Tâm ngắm
        let (cx, cy) = (w * 0.5, h * 0.5);
        quad(cx - 10.0 * s, cy - 1.0 * s, cx + 10.0 * s, cy + 1.0 * s, white);
        quad(cx - 1.0 * s, cy - 10.0 * s, cx + 1.0 * s, cy + 10.0 * s, white);

        // Joystick
        for ti in self.touches.values() {
            if ti.role == Role::Move {
                let r = 70.0 * s;
                let t = 2.0 * s;
                let (x, y) = (ti.start[0], ti.start[1]);
                let g = [0.9, 0.9, 0.9];
                quad(x - r, y - r, x + r, y - r + t, g);
                quad(x - r, y + r - t, x + r, y + r, g);
                quad(x - r, y - r, x - r + t, y + r, g);
                quad(x + r - t, y - r, x + r, y + r, g);
                let dx = (ti.cur[0] - x).clamp(-r, r);
                let dy = (ti.cur[1] - y).clamp(-r, r);
                let k = 14.0 * s;
                quad(x + dx - k, y + dy - k, x + dx + k, y + dy + k, white);
            }
        }

        // Ô block đang chọn (góc trên-phải)
        let c = Block::PLACEABLE[self.selected].color(2);
        let (x1, y0) = (w - 16.0 * s, 16.0 * s);
        let size = 44.0 * s;
        quad(x1 - size - 3.0 * s, y0 - 3.0 * s, x1 + 3.0 * s, y0 + size + 3.0 * s, white);
        quad(x1 - size, y0, x1, y0 + size, c);

        v.truncate(UI_MAX_QUADS * 6 * 6);
        v
    }

    // ---------- render ----------

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let eye = self.player.eye();
        let proj = math::perspective(70f32.to_radians(), aspect, 0.1, 400.0);
        let viewm = math::look_to(eye, self.player.dir());
        let far = (RADIUS * CS) as f32;
        let u = Uniforms {
            mvp: math::mul(&proj, &viewm),
            cam: [eye[0], eye[1], eye[2], 0.0],
            fog: [far * 0.55, far * 0.95, 1.0, 0.0],
        };
        self.queue.write_buffer(&self.ubuf_world, 0, bytemuck::bytes_of(&u));

        let ui = self.build_ui();
        self.queue.write_buffer(&self.ui_vb, 0, bytemuck::cast_slice(&ui));
        let ui_verts = (ui.len() / 6) as u32;

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
                    depth_slice: None,
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
            if ui_verts > 0 {
                pass.set_bind_group(0, &self.bg_ui, &[]);
                pass.set_vertex_buffer(0, self.ui_vb.slice(0..(ui_verts as u64 * 24)));
                pass.draw(0..ui_verts, 0..1);
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
