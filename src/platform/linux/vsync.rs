use crate::renderer::VSync;

pub fn uses_winit_throttling(vsync: &VSync) -> bool {
    matches!(vsync, VSync::WinitThrottling())
}
