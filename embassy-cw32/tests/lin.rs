//! Host-executed protocol tests. No peripheral acquisition or MMIO.
#![cfg(any(
    feature = "cw32l010f8p6",
    feature = "cw32l011k8t6",
    feature = "cw32l012c8t6"
))]

use embassy_cw32::usart::*;

fn id(value: u8) -> LinIdentifier {
    LinIdentifier::new(value).unwrap()
}
fn frame(value: u8, len: usize) -> LinFrameDefinition {
    LinFrameDefinition::new(id(value), len, LinChecksum::Enhanced).unwrap()
}
fn entry(value: u8, ticks: u32) -> LinScheduleEntry {
    LinScheduleEntry {
        frame: frame(value, 8),
        time_base_ticks: ticks,
    }
}
fn timing() -> LinScheduleTiming {
    LinScheduleTiming::new(20_000, 1_000, 100).unwrap()
}

#[test]
fn pid_exhaustive_single_bit_corruption_and_reserved_ids() {
    for raw in 0..=61 {
        let identifier = id(raw);
        assert_eq!(
            LinIdentifier::from_protected(identifier.protected()),
            Ok(identifier)
        );
        for bit in 0..8 {
            assert!(LinIdentifier::from_protected(identifier.protected() ^ (1 << bit)).is_err());
        }
    }
    for raw in 62..=255 {
        assert!(LinIdentifier::new(raw).is_err());
    }
    assert_eq!(id(0).protected(), 0x80);
    assert_eq!(id(60).protected(), 0x3c);
    assert_eq!(id(61).protected(), 0x7d);
}

#[test]
fn checksum_known_values_carry_and_owned_storage() {
    let mut input = [0xff, 0xff, 1];
    let response = LinResponse::new(id(0), &input, LinChecksum::Classic).unwrap();
    input.fill(0);
    assert_eq!(response.as_bytes(), &[0xff, 0xff, 1, 0xfe]);
    let enhanced = LinResponse::new(id(0), &[0], LinChecksum::Enhanced).unwrap();
    assert_eq!(enhanced.as_bytes(), &[0, 0x7f]);
    assert_eq!(
        LinResponse::from_received(id(0), &[0, 0x7e], LinChecksum::Enhanced),
        Err(LinFrameError::ChecksumMismatch)
    );
    assert_eq!(
        LinResponse::from_received(id(0), enhanced.as_bytes(), LinChecksum::Enhanced),
        Ok(enhanced)
    );
}

#[test]
fn payload_and_diagnostic_limits() {
    for len in 0..=10 {
        let bytes = [0u8; 10];
        assert_eq!(
            LinResponse::new(id(12), &bytes[..len], LinChecksum::Classic).is_ok(),
            (1..=8).contains(&len)
        );
        for raw in [60, 61] {
            assert_eq!(
                LinResponse::new(id(raw), &bytes[..len], LinChecksum::Classic).is_ok(),
                len == 8
            );
            assert!(LinResponse::new(id(raw), &bytes[..len], LinChecksum::Enhanced).is_err());
        }
    }
    assert!(LinBreakBits::new(12).is_none());
    assert!(LinBreakBits::new(13).is_some());
    assert!(LinBreakBits::new(511).is_some());
    assert!(LinBreakBits::new(512).is_none());
}

#[test]
fn assembler_resynchronizes_and_rejects_corruption() {
    let mut receiver = LinFrameAssembler::new(id(12), 2, LinChecksum::Enhanced).unwrap();
    let response = LinResponse::new(id(12), &[1, 2], LinChecksum::Enhanced).unwrap();
    assert_eq!(receiver.push(LinRxEvent::Data(0x55)), Ok(None));
    receiver.push(LinRxEvent::Break).unwrap();
    assert_eq!(
        receiver.push(LinRxEvent::Data(0)),
        Err(LinAssemblyError::InvalidSync)
    );
    for event in [
        LinRxEvent::Break,
        LinRxEvent::Data(0x55),
        LinRxEvent::Data(id(12).protected()),
        LinRxEvent::Data(99),
        LinRxEvent::Break,
        LinRxEvent::Data(0x55),
        LinRxEvent::Data(id(12).protected()),
    ] {
        assert_eq!(receiver.push(event), Ok(None));
    }
    assert_eq!(receiver.push(LinRxEvent::Data(1)), Ok(None));
    assert_eq!(receiver.push(LinRxEvent::Data(2)), Ok(None));
    assert_eq!(
        receiver.push(LinRxEvent::Data(response.checksum() ^ 1)),
        Err(LinAssemblyError::Frame(LinFrameError::ChecksumMismatch))
    );
    receiver.reset();
    for byte in response.as_bytes() {
        assert_eq!(receiver.push(LinRxEvent::Data(*byte)), Ok(None));
    }
    for event in [
        LinRxEvent::Break,
        LinRxEvent::Data(0x55),
        LinRxEvent::Data(id(12).protected()),
        LinRxEvent::Data(1),
        LinRxEvent::Data(2),
    ] {
        receiver.push(event).unwrap();
    }
    let saved = receiver
        .push(LinRxEvent::Data(response.checksum()))
        .unwrap()
        .unwrap();
    receiver.reset();
    assert_eq!(saved, response);
}

#[test]
fn dispatch_and_response_tables_enforce_identity() {
    let duplicate = [frame(12, 1), frame(12, 8)];
    assert!(LinFrameDispatcher::new(&duplicate).is_err());
    let definitions = [frame(12, 1), frame(13, 8)];
    let mut dispatcher = LinFrameDispatcher::new(&definitions).unwrap();
    let response = LinResponse::new(id(13), &[4; 8], LinChecksum::Enhanced).unwrap();
    for event in [
        LinRxEvent::Break,
        LinRxEvent::Data(0x55),
        LinRxEvent::Data(id(14).protected()),
        LinRxEvent::Data(1),
        LinRxEvent::Break,
        LinRxEvent::Data(0x55),
        LinRxEvent::Data(id(13).protected()),
    ] {
        assert_eq!(dispatcher.push(event), Ok(None));
    }
    for byte in response.data() {
        assert_eq!(dispatcher.push(LinRxEvent::Data(*byte)), Ok(None));
    }
    assert_eq!(
        dispatcher.push(LinRxEvent::Data(response.checksum())),
        Ok(Some(response))
    );
    assert!(LinResponder::new(&[response, response]).is_err());
    let responses = [response];
    let mut responder = LinResponder::new(&responses).unwrap();
    for event in [LinRxEvent::Break, LinRxEvent::Data(0x55)] {
        assert_eq!(responder.push(event), Ok(None));
    }
    assert_eq!(
        responder.push(LinRxEvent::Data(id(13).protected())),
        Ok(Some(response))
    );
    assert_eq!(
        responder.push(LinRxEvent::Data(id(13).protected())),
        Ok(None)
    );
    responder.reset();
    assert_eq!(responder.push(LinRxEvent::Data(0x55)), Ok(None));
}

#[test]
fn schedule_strict_budget_and_overflow() {
    // 124 bits * 1.4 / 20000 = 8680 us, plus 100 us jitter; strict >.
    let exact = LinScheduleTiming::new(20_000, 1, 100).unwrap();
    assert_eq!(exact.minimum_slot_us(frame(12, 8)), 8781);
    assert!(LinSchedulePlan::new(exact, &[entry(12, 8780)]).is_err());
    assert!(LinSchedulePlan::new(exact, &[entry(12, 8781)]).is_ok());
    assert!(LinScheduleTiming::new(0, 1, 0).is_err());
    assert!(LinScheduleTiming::new(1, 0, 0).is_err());
    assert!(LinSchedulePlan::new(timing(), &[]).is_err());
    let huge = LinScheduleTiming::new(1, u32::MAX, 0).unwrap();
    assert!(matches!(
        LinSchedulePlan::new(huge, &[entry(12, u32::MAX); 2]),
        Err(LinScheduleError::DurationOverflow)
    ));
}

#[test]
fn cursor_deadlines_no_drift_and_overlap() {
    let entries = [entry(12, 10), entry(13, 12)];
    let plan = LinSchedulePlan::new(timing(), &entries).unwrap();
    let mut cursor = plan.cursor(1000);
    assert_eq!(cursor.start_slot(999), Ok(None));
    let first = cursor.start_slot(1100).unwrap().unwrap();
    assert_eq!((first.scheduled_start_us, first.end_us), (1000, 11000));
    assert_eq!(
        cursor.start_slot(1100),
        Err(LinScheduleRunError::SlotActive)
    );
    cursor.finish_slot(2000).unwrap();
    assert_eq!(cursor.next_slot_us(), 11000);
    let second = cursor.start_slot(11000).unwrap().unwrap();
    assert_eq!(second.index, 1);
    cursor.finish_slot(second.end_us).unwrap();
    assert_eq!(cursor.start_slot(23000).unwrap().unwrap().index, 0);
}

#[test]
fn cursor_faults_latch_until_explicit_recovery() {
    let entries = [entry(12, 10)];
    let plan = LinSchedulePlan::new(timing(), &entries).unwrap();
    let mut cursor = plan.cursor(0);
    assert_eq!(
        cursor.start_slot(101),
        Err(LinScheduleRunError::StartTooLate { index: 0 })
    );
    assert_eq!(
        cursor.start_slot(0),
        Err(LinScheduleRunError::StartTooLate { index: 0 })
    );
    cursor.restart(0);
    cursor.start_slot(0).unwrap();
    assert_eq!(
        cursor.finish_slot(10001),
        Err(LinScheduleRunError::FinishTooLate { index: 0 })
    );
    cursor.restart(10);
    cursor.start_slot(10).unwrap();
    assert_eq!(
        cursor.finish_slot(9),
        Err(LinScheduleRunError::ClockWentBackwards)
    );
    cursor.restart(u64::MAX);
    assert_eq!(
        cursor.start_slot(u64::MAX),
        Err(LinScheduleRunError::TimeOverflow)
    );
    cursor.restart(0);
    cursor.abort();
    assert_eq!(cursor.start_slot(0), Err(LinScheduleRunError::Aborted));
}

#[test]
fn switch_at_boundary_latest_wins_and_cancel_preserves_plan() {
    let a = [entry(12, 10)];
    let b = [entry(13, 12)];
    let c = [entry(14, 15)];
    let pa = LinSchedulePlan::new(timing(), &a).unwrap();
    let pb = LinSchedulePlan::new(timing(), &b).unwrap();
    let pc = LinSchedulePlan::new(timing(), &c).unwrap();
    let bad =
        LinSchedulePlan::new(LinScheduleTiming::new(19_200, 1_000, 100).unwrap(), &b).unwrap();
    let mut cursor = pa.cursor(0);
    cursor.start_slot(0).unwrap();
    cursor.request_plan(&pb).unwrap();
    assert_eq!(
        cursor.request_plan(&bad),
        Err(LinScheduleRunError::IncompatibleTiming)
    );
    cursor.request_plan(&pc).unwrap();
    cursor.finish_slot(1).unwrap();
    assert_eq!(cursor.start_slot(9999), Ok(None));
    let next = cursor.start_slot(10000).unwrap().unwrap();
    assert_eq!((next.frame.identifier(), next.end_us), (id(14), 25000));
    cursor.request_plan(&pb).unwrap();
    cursor.cancel_plan_change();
    cursor.finish_slot(25000).unwrap();
    assert_eq!(
        cursor
            .start_slot(25000)
            .unwrap()
            .unwrap()
            .frame
            .identifier(),
        id(14)
    );
    cursor.request_plan(&pa).unwrap();
    cursor.restart(0);
    assert_eq!(
        cursor.start_slot(0).unwrap().unwrap().frame.identifier(),
        id(14)
    );
}
