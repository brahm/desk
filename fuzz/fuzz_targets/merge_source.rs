// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sources of a mail merge (W105), which come from anywhere: any
//! bytes as a vCard file, a CSV file or a spreadsheet must give records,
//! or an error, without a panic, and every datum asked for must be one of
//! the records.

#![no_main]

use desk_contacts::{CONTACT_FIELDS, read_source};
use desk_model::MergeSource;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let sources = [
        MergeSource::VCard {
            file: "cards.vcf".into(),
        },
        MergeSource::Spreadsheet {
            file: "list.csv".into(),
            table: None,
        },
        MergeSource::Spreadsheet {
            file: "list.ods".into(),
            table: None,
        },
        MergeSource::Spreadsheet {
            file: "list.fods".into(),
            table: Some("Sheet1".into()),
        },
    ];
    for source in &sources {
        let Ok(records) = read_source(source, |_| Ok(data)) else {
            continue;
        };
        for row in &records.rows {
            assert_eq!(row.len(), records.fields.len());
        }
        for record in 0..records.len().min(4) {
            for field in CONTACT_FIELDS.iter().chain(records.fields.iter().map(|f| f.as_str()).collect::<Vec<_>>().iter()) {
                let _ = records.value(record, field);
            }
        }
    }
});
