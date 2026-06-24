use crate::cli::Cli;
use crate::config::{self, ApiKeyReference, Config, ModelsFile, ProvidersFile};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
pub struct CheckReport {
    pub successes: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl CheckReport {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn render(&self) -> String {
        let mut out = String::from("Cass config check\n");
        for success in &self.successes {
            out.push_str("✓ ");
            out.push_str(success);
            out.push('\n');
        }
        if !self.warnings.is_empty() {
            out.push_str("\nWarnings:\n");
            for warning in &self.warnings {
                out.push_str("! ");
                out.push_str(warning);
                out.push('\n');
            }
        }
        if !self.errors.is_empty() {
            out.push_str("\nErrors:\n");
            for error in &self.errors {
                out.push_str("✗ ");
                out.push_str(error);
                out.push('\n');
            }
        }
        let next_steps = self.next_steps();
        if !next_steps.is_empty() {
            out.push_str("\nNext step");
            if next_steps.len() != 1 {
                out.push('s');
            }
            out.push_str(":\n");
            for step in &next_steps {
                out.push_str("  ");
                out.push_str(step);
                out.push('\n');
            }
        }
        if !self.errors.is_empty() {
            out.push_str("\nConfig check failed.\n");
        } else {
            out.push_str("\nAll checks passed.\n");
        }
        out
    }

    fn next_steps(&self) -> Vec<String> {
        if self.errors.is_empty() {
            return Vec::new();
        }
        for error in &self.errors {
            if let Some(name) = missing_env_var_name(error) {
                return vec![
                    format!("export {name}=..."),
                    "cass check".into(),
                    "cass".into(),
                ];
            }
        }
        vec!["cass setup".into()]
    }
}

fn missing_env_var_name(error: &str) -> Option<String> {
    let marker = "environment variable `";
    let rest = error.split_once(marker)?.1;
    let name = rest.split_once('`')?.0;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

pub fn run(cli: &Cli) -> Result<CheckReport> {
    run_with_root(config::cass_root(), cli)
}

pub fn run_with_root(root: PathBuf, cli: &Cli) -> Result<CheckReport> {
    fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;

    let mut report = CheckReport::default();
    let config_path = config::config_path(&root);
    let providers_path = config::providers_path(&root);
    let models_path = config::models_path(&root);

    let config_file = match config::load_config_file(&root) {
        Ok(Some(file)) => {
            report
                .successes
                .push(format!("{}: valid", pretty_path(&config_path)));
            Some(file)
        }
        Ok(None) => {
            report.successes.push(format!(
                "{}: not present (using defaults)",
                pretty_path(&config_path)
            ));
            None
        }
        Err(err) => {
            report
                .errors
                .push(format!("{}: {err:#}", pretty_path(&config_path)));
            None
        }
    };

    let providers = match config::load_or_create_default_provider_registry(&root) {
        Ok(file) => {
            report.successes.push(format!(
                "{}: valid ({} provider{})",
                pretty_path(&providers_path),
                file.providers.len(),
                plural(file.providers.len())
            ));
            Some(file)
        }
        Err(err) => {
            report
                .errors
                .push(format!("{}: {err:#}", pretty_path(&providers_path)));
            None
        }
    };

    let models = match config::load_or_create_default_model_registry(&root) {
        Ok(file) => {
            report.successes.push(format!(
                "{}: valid ({} model{})",
                pretty_path(&models_path),
                file.models.len(),
                plural(file.models.len())
            ));
            Some(file)
        }
        Err(err) => {
            report
                .errors
                .push(format!("{}: {err:#}", pretty_path(&models_path)));
            None
        }
    };

    let (Some(providers), Some(models)) = (providers, models) else {
        return Ok(report);
    };

    if report.errors.is_empty() {
        validate(&mut report, config_file.as_ref(), &providers, &models);
    }

    if report.errors.is_empty() {
        match Config::load_from_root_with_docs(root.clone(), root.join("docs"), cli) {
            Ok(cfg) => check_active_config(&mut report, &cfg, &providers),
            Err(err) => report.errors.push(format!("active config: {err:#}")),
        }
    }

    Ok(report)
}

fn validate(
    report: &mut CheckReport,
    config_file: Option<&config::ConfigFile>,
    providers: &ProvidersFile,
    models: &ModelsFile,
) {
    let summary = config::validate_registries(config_file, providers, models);
    report.warnings.extend(summary.warnings);
    report.errors.extend(summary.errors);
}

fn check_active_config(report: &mut CheckReport, cfg: &Config, providers: &ProvidersFile) {
    report
        .successes
        .push(format!("active provider: {}", cfg.provider_id));
    report.successes.push(format!(
        "active provider base URL: {}",
        cfg.active_provider.base_url
    ));
    report
        .successes
        .push(format!("active model: {}", cfg.model));

    check_api_key(report, "api key", &cfg.active_provider.api_key, true);

    for provider in &providers.providers {
        if provider.id == cfg.provider_id {
            continue;
        }
        check_api_key(
            report,
            &format!("provider `{}` api key", provider.id),
            &provider.api_key,
            false,
        );
    }
}

fn check_api_key(report: &mut CheckReport, label: &str, spec: &str, active: bool) {
    match config::api_key_reference(spec) {
        Ok(ApiKeyReference::Env(name)) => match std::env::var(&name) {
            Ok(value) if !value.is_empty() => {
                if active {
                    report.successes.push(format!("{label}: {name} is set"));
                }
            }
            _ if active => report
                .errors
                .push(format!("{label}: environment variable `{name}` is not set")),
            _ => report
                .warnings
                .push(format!("{label}: environment variable `{name}` is not set")),
        },
        Ok(ApiKeyReference::Literal) => {
            if active {
                report
                    .successes
                    .push(format!("{label}: literal value configured"));
            }
        }
        Err(err) if active => report.errors.push(format!("{label}: {err}")),
        Err(err) => report.warnings.push(format!("{label}: {err}")),
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn pretty_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}
