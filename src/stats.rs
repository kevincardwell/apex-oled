use std::{ffi::c_void, fs, path::PathBuf};

/// Live system numbers. Only sampled while the system page is on screen.
pub struct Stats {
    prev: Vec<(u64, u64)>, // (busy, total) jiffies: [0] = all CPUs, [1..] = each thread
    pub load: Vec<f32>,    // same layout, 0..1
    pub cpu_temp: Option<f32>,
    pub mem_used_gib: f32,
    pub mem_total_gib: f32,
    pub gpu: Option<(u32, u32)>, // (temp °C, load %)
    temp_input: Option<PathBuf>,
    nvml: Option<Nvml>,
}

impl Stats {
    pub fn new() -> Self {
        Stats {
            prev: Vec::new(),
            load: Vec::new(),
            cpu_temp: None,
            mem_used_gib: 0.0,
            mem_total_gib: 0.0,
            gpu: None,
            temp_input: cpu_temp_input(),
            nvml: Nvml::load(),
        }
    }

    pub fn sample(&mut self) {
        let now = cpu_times(&fs::read_to_string("/proc/stat").unwrap_or_default());
        if now.len() == self.prev.len() {
            self.load = now
                .iter()
                .zip(&self.prev)
                .map(|(&(b, t), &(pb, pt))| if t > pt { (b - pb) as f32 / (t - pt) as f32 } else { 0.0 })
                .collect();
        }
        self.prev = now;

        self.cpu_temp = self
            .temp_input
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|m| m / 1000.0);

        let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let kib = |key: &str| {
            meminfo
                .lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        let gib = 1024.0 * 1024.0;
        self.mem_total_gib = kib("MemTotal:") / gib;
        self.mem_used_gib = self.mem_total_gib - kib("MemAvailable:") / gib;

        self.gpu = self.nvml.as_ref().and_then(Nvml::read);
    }
}

/// (busy, total) per `cpu` line of /proc/stat; iowait counts as idle.
fn cpu_times(stat: &str) -> Vec<(u64, u64)> {
    stat.lines()
        .take_while(|l| l.starts_with("cpu"))
        .map(|l| {
            let v: Vec<u64> = l.split_whitespace().skip(1).take(8).map(|x| x.parse().unwrap_or(0)).collect();
            let total: u64 = v.iter().sum();
            (total - v.get(3).unwrap_or(&0) - v.get(4).unwrap_or(&0), total)
        })
        .collect()
}

fn cpu_temp_input() -> Option<PathBuf> {
    fs::read_dir("/sys/class/hwmon")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let name = fs::read_to_string(p.join("name")).unwrap_or_default();
            matches!(name.trim(), "k10temp" | "zenpower" | "coretemp")
        })
        .map(|p| p.join("temp1_input"))
}

type InitFn = unsafe extern "C" fn() -> i32;
type ByIndexFn = unsafe extern "C" fn(u32, *mut *mut c_void) -> i32;
type TempFn = unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32;
type UtilFn = unsafe extern "C" fn(*mut c_void, *mut [u32; 2]) -> i32;

/// NVIDIA's management library, loaded at runtime so the binary still runs without it.
/// In-process calls instead of spawning nvidia-smi (~20 ms each).
struct Nvml {
    dev: *mut c_void,
    temp: TempFn,
    util: UtilFn,
}

impl Nvml {
    fn load() -> Option<Self> {
        unsafe {
            let lib = libc::dlopen(c"libnvidia-ml.so.1".as_ptr(), libc::RTLD_NOW);
            if lib.is_null() {
                return None;
            }
            let sym = |name: &std::ffi::CStr| {
                let p = libc::dlsym(lib, name.as_ptr());
                (!p.is_null()).then_some(p)
            };
            let init = std::mem::transmute::<*mut c_void, InitFn>(sym(c"nvmlInit_v2")?);
            let by_index = std::mem::transmute::<*mut c_void, ByIndexFn>(sym(c"nvmlDeviceGetHandleByIndex_v2")?);
            let mut dev = std::ptr::null_mut();
            if init() != 0 || by_index(0, &mut dev) != 0 {
                return None;
            }
            Some(Nvml {
                dev,
                temp: std::mem::transmute::<*mut c_void, TempFn>(sym(c"nvmlDeviceGetTemperature")?),
                util: std::mem::transmute::<*mut c_void, UtilFn>(sym(c"nvmlDeviceGetUtilizationRates")?),
            })
        }
    }

    fn read(&self) -> Option<(u32, u32)> {
        let (mut t, mut u) = (0, [0; 2]);
        unsafe { ((self.temp)(self.dev, 0, &mut t) == 0 && (self.util)(self.dev, &mut u) == 0).then_some((t, u[0])) }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpu_times_counts_iowait_as_idle() {
        let stat = "cpu  10 0 5 80 5 0 0 0 0 0\ncpu0 10 0 5 80 5 0 0 0 0 0\nintr 1";
        assert_eq!(super::cpu_times(stat), vec![(15, 100), (15, 100)]);
    }
}
