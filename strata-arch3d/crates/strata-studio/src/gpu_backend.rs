use eframe::egui_wgpu;
use std::sync::Arc;

/// 하드웨어 GPU 벤더 분류 (`NVIDIA` · `AMD` 외장 GPU 최우선 가속)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendorKind {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl GpuVendorKind {
    pub fn from_vendor_id_and_name(vendor_id: u32, name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if vendor_id == 0x10DE || lower.contains("nvidia") || lower.contains("geforce") || lower.contains("quadro") || lower.contains("rtx") || lower.contains("gtx") {
            Self::Nvidia
        } else if vendor_id == 0x1002 || vendor_id == 0x1022 || lower.contains("amd") || lower.contains("radeon") || lower.contains("firepro") {
            Self::Amd
        } else if vendor_id == 0x8086 || lower.contains("intel") {
            Self::Intel
        } else {
            Self::Other
        }
    }

    pub fn label_kr(self) -> &'static str {
        match self {
            Self::Nvidia => "NVIDIA 외장 GPU",
            Self::Amd => "AMD 외장 GPU",
            Self::Intel => "Intel GPU",
            Self::Other => "표준 GPU",
        }
    }
}

/// 탐지된 하드웨어 GPU 어댑터 요약 정보
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareGpuAdapterInfo {
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub vendor_kind: GpuVendorKind,
    pub backend_name: String,
    pub is_discrete: bool,
    pub driver_info: String,
    pub priority_score: i32,
}

impl HardwareGpuAdapterInfo {
    pub fn short_badge_text(&self) -> String {
        let kind_tag = if self.is_discrete { "외장" } else { "내장" };
        format!("⚡ GPU: {} ({} · {})", self.name, self.backend_name, kind_tag)
    }
}

/// 어댑터 우선순위 점수 산출 — NVIDIA (`0x10DE`) 및 AMD (`0x1002`) 외장 GPU(`DiscreteGpu`)를 최우선 선택한다.
pub fn score_gpu_adapter(info: &wgpu::AdapterInfo) -> i32 {
    let vendor = GpuVendorKind::from_vendor_id_and_name(info.vendor, &info.name);
    let mut score = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 10_000,
        wgpu::DeviceType::IntegratedGpu => 3_000,
        wgpu::DeviceType::VirtualGpu => 1_000,
        wgpu::DeviceType::Other => 500,
        wgpu::DeviceType::Cpu => -5_000,
    };

    score += match vendor {
        GpuVendorKind::Nvidia => 4_500,
        GpuVendorKind::Amd => 4_400,
        GpuVendorKind::Intel => 1_000,
        GpuVendorKind::Other => 500,
    };

    score += match info.backend {
        wgpu::Backend::Vulkan => 800,
        wgpu::Backend::Dx12 => 750,
        wgpu::Backend::Metal => 800,
        wgpu::Backend::Gl => 300,
        wgpu::Backend::BrowserWebGpu => 100,
        wgpu::Backend::Empty => -10_000,
    };

    score
}

/// 시스템에 장착된 모든 GPU(NVIDIA, AMD, Intel 등)를 탐지하여 우선순위 순으로 반환한다.
pub fn discover_hardware_gpus() -> Vec<HardwareGpuAdapterInfo> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12 | wgpu::Backends::GL,
        ..Default::default()
    });
    let adapters = instance.enumerate_adapters(wgpu::Backends::all());
    let mut list: Vec<HardwareGpuAdapterInfo> = adapters
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            let vendor_kind = GpuVendorKind::from_vendor_id_and_name(info.vendor, &info.name);
            let backend_name = match info.backend {
                wgpu::Backend::Vulkan => String::from("Vulkan"),
                wgpu::Backend::Dx12 => String::from("DirectX 12"),
                wgpu::Backend::Metal => String::from("Metal"),
                wgpu::Backend::Gl => String::from("OpenGL"),
                wgpu::Backend::BrowserWebGpu => String::from("WebGPU"),
                wgpu::Backend::Empty => String::from("Empty"),
            };
            let priority_score = score_gpu_adapter(&info);
            HardwareGpuAdapterInfo {
                name: info.name,
                vendor_id: info.vendor,
                device_id: info.device,
                vendor_kind,
                backend_name,
                is_discrete: info.device_type == wgpu::DeviceType::DiscreteGpu,
                driver_info: info.driver_info,
                priority_score,
            }
        })
        .collect();

    list.sort_by_key(|b| std::cmp::Reverse(b.priority_score));
    // 동일 이름+백엔드 중복 제거
    list.dedup_by(|a, b| a.name == b.name && a.backend_name == b.backend_name);
    list
}

/// 프레임마다 재탐지하지 않고 즉시 조회할 수 있도록 최우선 하드웨어 GPU 정보를 캐싱하여 반환한다.
pub fn primary_hardware_gpu() -> &'static Option<HardwareGpuAdapterInfo> {
    static PRIMARY_GPU: std::sync::OnceLock<Option<HardwareGpuAdapterInfo>> =
        std::sync::OnceLock::new();
    PRIMARY_GPU.get_or_init(|| discover_hardware_gpus().into_iter().next())
}

/// `eframe` `wgpu` 렌더러에서 NVIDIA / AMD 고성능 외장 GPU(`HighPerformance`)를 강제 선택하는 설정 생성
pub fn build_high_performance_wgpu_config(preferred_gpu_name: Option<String>) -> egui_wgpu::WgpuConfiguration {
    let selector: egui_wgpu::NativeAdapterSelectorMethod = Arc::new(move |adapters, _surface| {
        if adapters.is_empty() {
            return Err(String::from("사용 가능한 하드웨어 GPU 어댑터를 찾을 수 없습니다."));
        }
        if let Some(ref pref_name) = preferred_gpu_name {
            if let Some(matched) = adapters
                .iter()
                .find(|a| a.get_info().name.eq_ignore_ascii_case(pref_name))
            {
                return Ok(matched.clone());
            }
        }
        let mut best_idx = 0usize;
        let mut best_score = i32::MIN;
        for (i, adapter) in adapters.iter().enumerate() {
            let s = score_gpu_adapter(&adapter.get_info());
            if s > best_score {
                best_score = s;
                best_idx = i;
            }
        }
        Ok(adapters[best_idx].clone())
    });

    egui_wgpu::WgpuConfiguration {
        present_mode: wgpu::PresentMode::AutoNoVsync,
        desired_maximum_frame_latency: Some(1),
        wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(egui_wgpu::WgpuSetupCreateNew {
            instance_descriptor: wgpu::InstanceDescriptor {
                backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12 | wgpu::Backends::GL,
                ..Default::default()
            },
            power_preference: wgpu::PowerPreference::HighPerformance,
            native_adapter_selector: Some(selector),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nvidia_and_amd_discrete_gpu_scoring_and_discovery() {
        let nvidia_info = wgpu::AdapterInfo {
            name: String::from("NVIDIA GeForce RTX 3090"),
            vendor: 0x10DE,
            device: 0x2204,
            device_type: wgpu::DeviceType::DiscreteGpu,
            driver: String::from("NVIDIA"),
            driver_info: String::from("560.94"),
            backend: wgpu::Backend::Vulkan,
        };
        let amd_info = wgpu::AdapterInfo {
            name: String::from("AMD Radeon RX 7900 XTX"),
            vendor: 0x1002,
            device: 0x744C,
            device_type: wgpu::DeviceType::DiscreteGpu,
            driver: String::from("AMD"),
            driver_info: String::from("24.8.1"),
            backend: wgpu::Backend::Dx12,
        };
        let igpu_info = wgpu::AdapterInfo {
            name: String::from("Intel(R) UHD Graphics 770"),
            vendor: 0x8086,
            device: 0x4680,
            device_type: wgpu::DeviceType::IntegratedGpu,
            driver: String::from("Intel"),
            driver_info: String::from("31.0"),
            backend: wgpu::Backend::Dx12,
        };

        let s_nv = score_gpu_adapter(&nvidia_info);
        let s_amd = score_gpu_adapter(&amd_info);
        let s_igpu = score_gpu_adapter(&igpu_info);

        assert!(s_nv > s_igpu + 8_000, "NVIDIA discrete GPU must strongly outscore integrated GPU");
        assert!(s_amd > s_igpu + 8_000, "AMD discrete GPU must strongly outscore integrated GPU");

        let gpus = discover_hardware_gpus();
        assert!(!gpus.is_empty(), "must detect at least one GPU adapter on this machine");
    }
}
