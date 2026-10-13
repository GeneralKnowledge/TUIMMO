//! Small descriptive statistics for multi-seed experiment aggregates.

use serde::{Deserialize, Serialize};

/// Summary of a numeric metric across replicates (sample statistics).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NumericSummary {
    /// Replicates with a defined numeric value for this metric.
    pub n: u32,
    pub mean: Option<f64>,
    /// Sample standard deviation (`n >= 2`; Bessel correction).
    pub std_dev: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub median: Option<f64>,
    pub p25: Option<f64>,
    pub p75: Option<f64>,
}

/// Summary of an optional rate across replicates (undefined rates excluded from distribution).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RateSummary {
    pub n_defined: u32,
    pub n_undefined: u32,
    pub distribution: NumericSummary,
    /// Pooled numerator/denominator across all replicates (distinct from mean of per-run rates).
    pub pooled_rate: Option<f64>,
    pub pooled_numerator: u64,
    pub pooled_denominator: u64,
}

pub fn numeric_summary(values: &[f64]) -> NumericSummary {
    let n = values.len();
    if n == 0 {
        return NumericSummary {
            n: 0,
            mean: None,
            std_dev: None,
            min: None,
            max: None,
            median: None,
            p25: None,
            p75: None,
        };
    }
    let n_u32 = u32::try_from(n).unwrap_or(u32::MAX);
    let mean = values.iter().sum::<f64>() / n as f64;
    let std_dev = sample_std_dev(values, mean);
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    NumericSummary {
        n: n_u32,
        mean: Some(mean),
        std_dev,
        min: sorted.first().copied(),
        max: sorted.last().copied(),
        median: Some(percentile_nearest(&sorted, 0.5)),
        p25: Some(percentile_nearest(&sorted, 0.25)),
        p75: Some(percentile_nearest(&sorted, 0.75)),
    }
}

pub fn rate_summary(per_run: &[Option<f64>], pooled_num: u64, pooled_den: u64) -> RateSummary {
    let defined: Vec<f64> = per_run.iter().filter_map(|v| *v).collect();
    let n_undefined = per_run.len().saturating_sub(defined.len()) as u32;
    RateSummary {
        n_defined: defined.len() as u32,
        n_undefined,
        distribution: numeric_summary(&defined),
        pooled_rate: if pooled_den == 0 {
            None
        } else {
            Some(pooled_num as f64 / pooled_den as f64)
        },
        pooled_numerator: pooled_num,
        pooled_denominator: pooled_den,
    }
}

fn sample_std_dev(values: &[f64], mean: f64) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let variance = values
        .iter()
        .map(|v| {
            let d = v - mean;
            d * d
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    Some(variance.sqrt())
}

/// Nearest-rank percentile on a sorted slice.
fn percentile_nearest(sorted: &[f64], p: f64) -> f64 {
    assert!(!sorted.is_empty());
    let rank = (p * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_summary_hand_verified() {
        let s = numeric_summary(&[2.0, 4.0, 6.0]);
        assert_eq!(s.n, 3);
        assert_eq!(s.mean, Some(4.0));
        assert_eq!(s.min, Some(2.0));
        assert_eq!(s.max, Some(6.0));
        assert_eq!(s.median, Some(4.0));
        let std = s.std_dev.expect("sample std dev");
        assert!((std - 2.0).abs() < 1e-9);
    }

    #[test]
    fn single_value_has_no_sample_std_dev() {
        let s = numeric_summary(&[7.0]);
        assert_eq!(s.n, 1);
        assert_eq!(s.mean, Some(7.0));
        assert!(s.std_dev.is_none());
    }
}
