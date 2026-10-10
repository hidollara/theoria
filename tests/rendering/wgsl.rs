use theoria_color::{IccProfile, IccRenderingIntent};
use theoria_renderer::transfer_wgsl::TransferWgslExt;

#[test]
fn icc_profile_transfer_generates_valid_wgsl() {
    let bytes = include_bytes!("../../crates/theoria-color/src/color/fixtures/sRGB-v4.icc");
    let profile = IccProfile::from_bytes(bytes).unwrap();
    let space = profile.color_space(IccRenderingIntent::RelativeColorimetric);
    let transfer = space.transfer_to_connection_space().unwrap();
    let shader = transfer.wgsl_with_storage(22, 1, 4).unwrap();
    let module = naga::front::wgsl::parse_str(shader.source()).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
