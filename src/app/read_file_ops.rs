use std::path::PathBuf;

use serde::Deserialize;

use crate::generators::internal::InternalPattern;

use super::PGenApp;

#[derive(Deserialize)]
struct CsvPatchRecord {
    _idx: usize,
    red: u16,
    green: u16,
    blue: u16,
    _label: Option<String>,
}

pub fn parse_patch_list_csv_file(app: &mut PGenApp, path: PathBuf) {
    let reader = csv::ReaderBuilder::new().has_headers(false).from_path(path);

    if let Ok(mut rdr) = reader {
        let pattern_iter = rdr
            .deserialize::<CsvPatchRecord>()
            .filter_map(Result::ok)
            .map(Into::into);

        app.cal_state.internal_gen.list.clear();
        app.cal_state.internal_gen.list.extend(pattern_iter);

        log::trace!(
            "Patch list CSV loaded: {} patches.",
            app.cal_state.internal_gen.list.len()
        );
    }
}

impl From<CsvPatchRecord> for InternalPattern {
    fn from(record: CsvPatchRecord) -> Self {
        Self {
            rgb: [record.red, record.green, record.blue],
            ..Default::default()
        }
    }
}
