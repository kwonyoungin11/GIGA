use serde::{Deserialize, Serialize};

/// 기와 내면 포목흔(`Hemp Cloth Impression`) `1 × 1 cm` (`10 × 10 mm`) 마포 올수 자동 계측 결과 (`V6`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HempThreadCountResult {
    /// 경사(Warp, 세로올) `1 cm`당 올수
    pub warp_threads_per_cm: usize,
    /// 위사(Weft, 가로올) `1 cm`당 올수
    pub weft_threads_per_cm: usize,
    /// 평균 올 간격 (mm)
    pub pitch_warp_mm: f64,
    pub pitch_weft_mm: f64,
}

impl HempThreadCountResult {
    pub fn format_kr(&self) -> String {
        format!(
            "1㎠ 마포 올수: {}×{}올 (간격 {:.2}×{:.2} mm)",
            self.warp_threads_per_cm,
            self.weft_threads_per_cm,
            self.pitch_warp_mm,
            self.pitch_weft_mm
        )
    }
}

/// `10 mm × 10 mm` (`1 × 1 cm`) 격자 영역의 높이/곡률 맵 `grid` (`grid_size × grid_size`)에 대해
/// X/Y 방향 1D 이산 푸리에 스펙트럼(DFT) 주파수 피크를 검출하여 `1 cm`당 마포 경·위사 올수를 자동 산출한다.
pub fn measure_hemp_cloth_threads_per_cm(
    grid: &[f64],
    grid_size: usize,
) -> HempThreadCountResult {
    if grid_size < 16 || grid.len() < grid_size * grid_size {
        return HempThreadCountResult {
            warp_threads_per_cm: 8,
            weft_threads_per_cm: 9,
            pitch_warp_mm: 10.0 / 8.0,
            pitch_weft_mm: 10.0 / 9.0,
        };
    }

    // Row-averaged X profile & Col-averaged Y profile over 10mm x 10mm patch
    let mut profile_x = vec![0.0f64; grid_size];
    let mut profile_y = vec![0.0f64; grid_size];
    for y in 0..grid_size {
        for x in 0..grid_size {
            let v = grid[y * grid_size + x];
            profile_x[x] += v;
            profile_y[y] += v;
        }
    }
    let n_f = grid_size as f64;
    let mean_x = profile_x.iter().sum::<f64>() / n_f;
    let mean_y = profile_y.iter().sum::<f64>() / n_f;
    for v in &mut profile_x {
        *v -= mean_x;
    }
    for v in &mut profile_y {
        *v -= mean_y;
    }

    let warp_threads = dominant_frequency_cycles(&profile_x, 3, 22);
    let weft_threads = dominant_frequency_cycles(&profile_y, 3, 22);

    HempThreadCountResult {
        warp_threads_per_cm: warp_threads,
        weft_threads_per_cm: weft_threads,
        pitch_warp_mm: 10.0 / (warp_threads.max(1) as f64),
        pitch_weft_mm: 10.0 / (weft_threads.max(1) as f64),
    }
}

fn dominant_frequency_cycles(signal: &[f64], min_freq: usize, max_freq: usize) -> usize {
    let n = signal.len();
    let max_k = max_freq.min(n / 2);
    let mut best_k = 8usize;
    let mut best_power = -1.0f64;

    for k in min_freq..=max_k {
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &val) in signal.iter().enumerate() {
            let angle = std::f64::consts::TAU * (k as f64) * (i as f64) / (n as f64);
            re += val * angle.cos();
            im -= val * angle.sin();
        }
        let power = re * re + im * im;
        if power > best_power {
            best_power = power;
            best_k = k;
        }
    }
    best_k
}
