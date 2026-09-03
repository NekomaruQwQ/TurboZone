use std::cell::Cell;

use smol_str::SmolStr;

use super::ProgramDescriptions;

#[test]
fn cache_reuses_description_for_case_insensitive_program_path() {
    let lookups = Cell::new(0);
    let mut descriptions = ProgramDescriptions::default();
    let program_name = SmolStr::new_static("app.exe");
    let first = descriptions.get_or_insert_with(
        "C:/Apps/App.exe",
        &program_name,
        || {
            lookups.set(lookups.get() + 1);
            Some(SmolStr::new_static("Application"))
        });
    let second = descriptions.get_or_insert_with(
        "c:/apps/app.EXE",
        &program_name,
        || {
            lookups.set(lookups.get() + 1);
            Some(SmolStr::new_static("Changed"))
        });

    assert_eq!((first.as_str(), second.as_str(), lookups.get()), ("Application", "Application", 1));
}

#[test]
fn cache_uses_program_name_when_description_is_unavailable() {
    let mut descriptions = ProgramDescriptions::default();
    let program_name = SmolStr::new_static("app.exe");

    let description = descriptions.get_or_insert_with(
        "C:/Apps/App.exe",
        &program_name,
        || None);

    assert_eq!(description, program_name);
}

#[test]
fn cache_uses_program_name_when_description_is_empty() {
    let mut descriptions = ProgramDescriptions::default();
    let program_name = SmolStr::new_static("app.exe");

    let description = descriptions.get_or_insert_with(
        "C:/Apps/App.exe",
        &program_name,
        || Some(SmolStr::new_static("")));

    assert_eq!(description, program_name);
}

#[test]
fn cache_evicts_program_paths_not_observed_in_the_latest_snapshot() {
    let mut descriptions = ProgramDescriptions::default();
    let program_name = SmolStr::new_static("app.exe");
    descriptions.get_or_insert_with(
        "C:/Apps/Retained.exe",
        &program_name,
        || Some(SmolStr::new_static("Retained")));
    descriptions.get_or_insert_with(
        "C:/Apps/Evicted.exe",
        &program_name,
        || Some(SmolStr::new_static("Old")));

    descriptions.retain_observed(std::iter::once("c:/apps/retained.EXE"));
    let retained = descriptions.get_or_insert_with(
        "C:/Apps/Retained.exe",
        &program_name,
        || Some(SmolStr::new_static("Changed")));
    let refreshed = descriptions.get_or_insert_with(
        "C:/Apps/Evicted.exe",
        &program_name,
        || Some(SmolStr::new_static("Refreshed")));

    assert_eq!((retained.as_str(), refreshed.as_str()), ("Retained", "Refreshed"));
}
