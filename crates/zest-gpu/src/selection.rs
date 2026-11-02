use crate::{RenderError, error};
use std::{fmt, str::FromStr};

/// Native API preference. Auto tries the platform's backends in priority order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    #[default]
    Auto,
    Dx12,
    Vulkan,
    Metal,
}
impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::Dx12 => "dx12",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
        })
    }
}
impl FromStr for Backend {
    type Err = RenderError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "dx12" => Ok(Self::Dx12),
            "vulkan" => Ok(Self::Vulkan),
            "metal" => Ok(Self::Metal),
            _ => Err(error(
                "backend",
                format!("unknown choice {value:?}; expected auto, dx12, vulkan or metal"),
            )),
        }
    }
}

/// An initialization failure retained when Auto succeeds with a later backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendFailure {
    pub backend: Backend,
    pub error: RenderError,
}

pub(crate) fn compiled_backends() -> &'static [Backend] {
    #[cfg(target_os = "windows")]
    {
        &[Backend::Dx12, Backend::Vulkan]
    }
    #[cfg(target_vendor = "apple")]
    {
        &[Backend::Metal]
    }
    #[cfg(all(not(target_os = "windows"), not(target_vendor = "apple")))]
    {
        &[Backend::Vulkan]
    }
}

pub(crate) fn select<T>(
    preference: Backend,
    available: &[Backend],
    mut create: impl FnMut(Backend) -> Result<T, RenderError>,
) -> Result<(T, Vec<BackendFailure>), RenderError> {
    if preference != Backend::Auto && !available.contains(&preference) {
        return Err(error(
            "backend selection",
            format!(
                "{preference} is not compiled for this target; available: {}",
                available
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    let explicit = [preference];
    let candidates = if preference == Backend::Auto {
        available
    } else {
        &explicit
    };
    let mut failures = Vec::new();
    for &backend in candidates {
        match create(backend) {
            Ok(renderer) => return Ok((renderer, failures)),
            Err(error) => failures.push(BackendFailure { backend, error }),
        }
    }
    if failures.is_empty() {
        return Err(error(
            "backend selection",
            "no native backend compiled for this target",
        ));
    }
    Err(error(
        "GPU initialization",
        failures
            .iter()
            .map(|f| format!("{}: {}", f.backend, f.error))
            .collect::<Vec<_>>()
            .join("; "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    const WINDOWS: &[Backend] = &[Backend::Dx12, Backend::Vulkan];
    #[test]
    fn auto_stops_at_first_success_and_retains_failed_attempts() {
        let mut calls = Vec::new();
        let (chosen, failures) = select(Backend::Auto, WINDOWS, |backend| {
            calls.push(backend);
            if backend == Backend::Dx12 {
                Err(error("adapter", "unavailable"))
            } else {
                Ok(backend)
            }
        })
        .unwrap();
        assert_eq!(calls, WINDOWS);
        assert_eq!(chosen, Backend::Vulkan);
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0]
                .error
                .to_string()
                .contains("adapter: unavailable")
        );
        calls.clear();
        let (chosen, failures) = select(Backend::Auto, WINDOWS, |backend| {
            calls.push(backend);
            Ok(backend)
        })
        .unwrap();
        assert_eq!(chosen, Backend::Dx12);
        assert_eq!(calls, [Backend::Dx12]);
        assert!(failures.is_empty());
    }
    #[test]
    fn explicit_choice_never_silently_falls_back() {
        let mut calls = Vec::new();
        let result = select::<()>(Backend::Dx12, WINDOWS, |backend| {
            calls.push(backend);
            Err(error("shader", "failed"))
        });
        assert_eq!(calls, [Backend::Dx12]);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("dx12: shader: failed")
        );
    }
    #[test]
    fn unsupported_choice_is_rejected_without_creating_a_backend() {
        let mut called = false;
        let result = select(Backend::Metal, WINDOWS, |_| {
            called = true;
            Ok(())
        });
        assert!(!called);
        assert!(result.unwrap_err().to_string().contains("not compiled"));
    }
    #[test]
    fn all_failures_and_stage_diagnostics_survive() {
        let error = select::<()>(Backend::Auto, WINDOWS, |b| Err(error("instance", b)))
            .unwrap_err()
            .to_string();
        assert!(error.contains("dx12: instance: dx12"));
        assert!(error.contains("vulkan: instance: vulkan"));
        assert!(
            select(Backend::Auto, &[], |_| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("no native backend")
        );
    }
    #[test]
    fn parsing_and_platform_candidates_are_explicit() {
        assert_eq!("DX12".parse::<Backend>().unwrap(), Backend::Dx12);
        assert!("unknown".parse::<Backend>().is_err());
        assert!(!compiled_backends().contains(&Backend::Auto));
        #[cfg(target_os = "windows")]
        assert_eq!(compiled_backends(), WINDOWS);
        #[cfg(target_vendor = "apple")]
        assert_eq!(compiled_backends(), [Backend::Metal]);
        #[cfg(all(not(target_os = "windows"), not(target_vendor = "apple")))]
        assert_eq!(compiled_backends(), [Backend::Vulkan]);
    }
}
