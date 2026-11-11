# Cubeland

Game voxel sandbox viết bằng Rust (`wgpu` + `winit`), chạy trên iPhone qua SideStore.
Code mới hoàn toàn, không dùng code hay asset của Mojang.

## Điều khiển (cảm ứng)

| Thao tác | Tác dụng |
|---|---|
| Kéo ở nửa trái | Joystick di chuyển |
| Chạm nhanh nửa trái | Nhảy (đi vào bậc cao 1 ô thì tự nhảy) |
| Kéo ở nửa phải | Xoay camera |
| Chạm nhanh nửa phải | Phá block đang nhắm |
| Chạm nhanh góc dưới-phải | Đặt block |
| Chạm nhanh góc trên-phải | Đổi loại block (ô màu hiện ở góc) |

## Cấu trúc

- `src/world.rs` – block, chunk, sinh địa hình (noise), cây, raycast
- `src/mesh.rs` – dựng mesh, chỉ vẽ mặt lộ ra
- `src/player.rs` – trọng lực, nhảy, va chạm AABB
- `src/math.rs` – ma trận
- `src/main.rs` + `src/shader.wgsl` – render, input cảm ứng
- `ios/Info.plist` – thông tin app iOS
- `.github/workflows/build-ipa.yml` – build ra `Cubeland.ipa`

Phần lõi (`world`, `mesh`, `player`, `math`) chỉ dùng thư viện chuẩn, có 19 test: `cargo test --lib`.

## Build .ipa

1. Đẩy repo lên GitHub (nhánh `main`).
2. Vào tab **Actions → Build iOS IPA → Run workflow** (hoặc cứ push là tự chạy).
3. Xong, tải artifact **Cubeland-ipa** (giải nén ra `Cubeland.ipa`).
   Muốn link tải trực tiếp: đẩy tag `v0.1.0`, file sẽ nằm ở mục Releases.
4. Mở `Cubeland.ipa` bằng SideStore để cài.

## Hướng phát triển tiếp

Lưu/tải thế giới, nước, ánh sáng theo block, texture atlas, mob, crafting, redstone...
