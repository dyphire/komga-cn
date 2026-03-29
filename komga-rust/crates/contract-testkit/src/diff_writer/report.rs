use crate::DiffReport;
use anyhow::Context;
use std::fs;
use std::path::Path;

pub fn write_diff_report(output_dir: &Path, report: &DiffReport) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create diff output dir {}", output_dir.display()))?;
    let report_path = output_dir.join(format!("{}.json", report.case_id));
    let report_json =
        serde_json::to_string_pretty(report).context("failed to serialize diff report")?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("failed to write diff report {}", report_path.display()))
}
