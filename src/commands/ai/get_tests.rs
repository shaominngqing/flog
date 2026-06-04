use super::*;

#[test]
fn parse_record_id_accepts_log_net_and_chunk() {
    assert_eq!(parse_record_id("log#12").unwrap(), RecordId::Log(12));
    assert_eq!(parse_record_id("net#42").unwrap(), RecordId::Net(42));
    assert_eq!(
        parse_record_id("chunk#42.13").unwrap(),
        RecordId::Chunk {
            net_id: 42,
            chunk: 13
        }
    );
}

#[test]
fn parse_record_id_rejects_unknown_shape() {
    assert!(parse_record_id("request#1").is_err());
    assert!(parse_record_id("chunk#x.y").is_err());
}
