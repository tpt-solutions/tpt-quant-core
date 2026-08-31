// SPDX-License-Identifier: MIT OR Apache-2.0

//! Running calibration statistics (requires the `alloc` feature).
//!
//! [`RunningStats`] accumulates a stream of `f32` values and answers percentile
//! queries without retaining the raw samples — only `min`, `max`, `count`, and a
//! fixed-bucket histogram are stored. This is what a caller uses to pick, say,
//! the 99th-percentile abs-max as a calibration scale before quantizing a whole
//! tensor.
//!
//! ## Histogram strategy: fixed-bucket over an adaptive range, outliers clamped
//!
//! The histogram has a fixed number of buckets (`BUCKETS`). The range `[lo, hi]`
//! is established from the first batch of values seen and then held fixed; values
//! outside that range are clamped into the first/last bucket. The tradeoff:
//! percentiles for values within the initial observed range are accurate to
//! within one bucket width, while percentiles in the clamped tail are
//! approximate (all such values are coalesced into the edge bucket). This keeps
//! memory O(1) and update O(n) with no unbounded storage, at the cost of tail
//! accuracy — exactly the right trade for calibration, where the bulk of the
//! distribution matters far more than extreme outliers.

use alloc::vec;
use alloc::vec::Vec;

/// Number of histogram buckets. Fixed for the lifetime of the program.
const BUCKETS: usize = 1024;

/// Running min/max/count plus a fixed-bucket histogram for percentile estimation.
///
/// Only available with the `alloc` feature.
#[derive(Debug, Clone)]
pub struct RunningStats {
    count: usize,
    min: f32,
    max: f32,
    lo: f32,
    hi: f32,
    buckets: Vec<usize>,
}

impl Default for RunningStats {
    fn default() -> Self {
        Self::new()
    }
}

impl RunningStats {
    /// Create an empty accumulator.
    pub fn new() -> Self {
        RunningStats {
            count: 0,
            min: f32::INFINITY,
            max: f32::NEG_INFINITY,
            lo: 0.0,
            hi: 0.0,
            buckets: vec![0usize; BUCKETS],
        }
    }

    /// Total number of values seen so far.
    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    /// Minimum value observed (or `f32::INFINITY` if empty).
    #[inline]
    pub fn min(&self) -> f32 {
        self.min
    }

    /// Maximum value observed (or `f32::NEG_INFINITY` if empty).
    #[inline]
    pub fn max(&self) -> f32 {
        self.max
    }

    /// Fold a batch of `values` into the running statistics.
    pub fn update(&mut self, values: &[f32]) {
        if values.is_empty() {
            return;
        }

        if self.count == 0 {
            // Establish the initial histogram range from this batch.
            let mut bmin = values[0];
            let mut bmax = values[0];
            for &v in &values[1..] {
                if v < bmin {
                    bmin = v;
                }
                if v > bmax {
                    bmax = v;
                }
            }
            self.lo = bmin;
            self.hi = bmax;
            if self.hi <= self.lo {
                // Degenerate batch: widen by one to give the histogram width.
                self.hi = self.lo + 1.0;
                self.lo -= 1.0;
            }
        }

        let width = (self.hi - self.lo) / BUCKETS as f32;
        for &v in values {
            if v < self.min {
                self.min = v;
            }
            if v > self.max {
                self.max = v;
            }
            let mut idx = ((v - self.lo) / width) as usize;
            if idx >= BUCKETS {
                idx = BUCKETS - 1; // clamp outliers into the top bucket
            }
            self.buckets[idx] += 1;
        }
        self.count += values.len();
    }

    /// Estimate the `p`-th percentile (`p` in `[0, 1]`) via the histogram.
    ///
    /// Returns `0.0` for an empty accumulator. Within the initial range the
    /// estimate is accurate to within one bucket width; in the clamped tail it is
    /// approximate (see module docs).
    pub fn percentile(&self, p: f32) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let p = p.clamp(0.0, 1.0);
        let width = (self.hi - self.lo) / BUCKETS as f32;
        let target = p * (self.count as f32 - 1.0);

        let mut cum: usize = 0;
        for (i, &c) in self.buckets.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let prev = cum;
            cum += c;
            if cum as f32 >= target {
                let frac = if c == 0 {
                    0.0
                } else {
                    (target - prev as f32) / c as f32
                };
                let bucket_lo = self.lo + i as f32 * width;
                return bucket_lo + frac * width;
            }
        }
        self.hi
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn uniform_distribution_median() {
        let mut s = RunningStats::new();
        let data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        s.update(&data);
        assert!((s.percentile(0.5) - 500.0).abs() < 1.0);
        assert!((s.percentile(0.0) - 0.0).abs() < 1.0);
        assert!((s.percentile(1.0) - 999.0).abs() < 1.0);
    }

    #[test]
    fn all_equal_distribution() {
        let mut s = RunningStats::new();
        let data = [3.0f32; 200];
        s.update(&data);
        assert_eq!(s.min(), 3.0);
        assert_eq!(s.max(), 3.0);
        // All values land in one bucket; interpolation puts the estimate within
        // one bucket width of the true value.
        assert!((s.percentile(0.5) - 3.0).abs() < 2.0 / BUCKETS as f32);
    }

    #[test]
    fn single_outlier_does_not_distort_bulk() {
        let mut s = RunningStats::new();
        let mut data: Vec<f32> = (0..1000).map(|i| (i as f32) / 1000.0).collect();
        // First batch sets the range [0, 0.999]. A later huge outlier is clamped.
        s.update(&data);
        data.clear();
        data.push(1e9);
        s.update(&data);
        // Bulk percentiles should still be accurate within one bucket width.
        assert!((s.percentile(0.5) - 0.5).abs() < (1.0 / BUCKETS as f32) + 1e-3);
        assert_eq!(s.max(), 1e9);
    }

    #[test]
    fn count_tracks_total() {
        let mut s = RunningStats::new();
        s.update(&[1.0, 2.0, 3.0]);
        s.update(&[4.0]);
        assert_eq!(s.count(), 4);
    }

    #[test]
    fn empty_is_safe() {
        let s = RunningStats::new();
        assert_eq!(s.count(), 0);
        assert_eq!(s.percentile(0.5), 0.0);
    }
}
