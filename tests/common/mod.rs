//! Shared fixtures and comparator for the signed text-baseline gate.
//! No formula is recomputed here: baseline and box come from render's real
//! parse_ir -> layout -> text_spans path, driven by the signed per-em table.
pub mod text_baseline_metrics;
