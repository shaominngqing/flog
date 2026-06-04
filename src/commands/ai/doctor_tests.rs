use super::*;

#[test]
fn port_range_defaults_to_ten_ports() {
    assert_eq!(
        port_range(9753),
        vec![9753, 9754, 9755, 9756, 9757, 9758, 9759, 9760, 9761, 9762]
    );
}

#[test]
fn command_status_maps_not_found() {
    let status = command_status_from_error(std::io::ErrorKind::NotFound);
    assert_eq!(status, CheckStatus::Missing);
}
