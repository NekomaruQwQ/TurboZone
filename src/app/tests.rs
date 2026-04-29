use super::*;

#[test]
fn tick_schedule_starts_due() {
    let now = Instant::now();
    let schedule = TickSchedule::immediate(now, Duration::from_millis(100));

    assert!(schedule.is_due(now));
    assert_eq!(schedule.remaining(now), Duration::ZERO);
}

#[test]
fn tick_schedule_waits_for_full_interval_after_restart() {
    let now = Instant::now();
    let interval = Duration::from_millis(100);
    let mut schedule = TickSchedule::immediate(now, interval);

    schedule.restart(now);

    assert!(!schedule.is_due(now + Duration::from_millis(99)));
    assert!(schedule.is_due(now + interval));
}

#[test]
fn tick_schedule_skips_missed_ticks_after_a_stall() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);
    let delayed = start + Duration::from_secs(1);
    let mut schedule = TickSchedule::immediate(start, interval);

    schedule.restart(delayed);

    assert_eq!(schedule.remaining(delayed), interval);
    assert!(!schedule.is_due(delayed));
}
