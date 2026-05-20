//! Reusable CLI UI primitives: theming, prompts, and output formatting.

use std::env;
use std::fmt::Write;

use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Password, Select};

pub fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

pub fn select<T: ToString>(prompt: &str, items: &[T]) -> anyhow::Result<usize> {
    if items.is_empty() {
        anyhow::bail!("ui::select called with no items for prompt {prompt:?}");
    }
    let string_items: Vec<String> = items.iter().map(|i| i.to_string()).collect();
    let choice = Select::with_theme(&theme())
        .with_prompt(prompt)
        .items(&string_items)
        .default(0)
        .interact()?;
    Ok(choice)
}

pub fn input(prompt: &str) -> anyhow::Result<String> {
    let value = Input::with_theme(&theme())
        .with_prompt(prompt)
        .interact_text()?;
    Ok(value)
}

pub fn input_with_default(prompt: &str, default: impl Into<String>) -> anyhow::Result<String> {
    let value = Input::with_theme(&theme())
        .with_prompt(prompt)
        .default(default.into())
        .interact_text()?;
    Ok(value)
}

pub fn password(prompt: &str) -> anyhow::Result<String> {
    let value = Password::with_theme(&theme())
        .with_prompt(prompt)
        .interact()?;
    Ok(value)
}

#[allow(dead_code)]
pub fn section(title: &str) {
    println!("\n{title}\n");
}

pub fn header(title: &str) {
    println!("{title}\n");
}

pub fn step(message: &str) {
    println!("  {message}");
}

pub fn done(message: &str) {
    println!("\n{message}");
}

pub fn status_line(icon: &str, label: &str, value: &str) -> String {
    format!("[{icon}] {label:<13} {value}")
}

pub fn report_section(title: &str, lines: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(&mut out, "{title}\n");
    for line in lines {
        let _ = writeln!(&mut out, "{line}");
    }
    out
}

#[allow(dead_code)]
pub fn use_color() -> bool {
    if env::var("NO_COLOR").is_ok() {
        return false;
    }
    if let Ok(v) = env::var("SOTTO_COLOR") {
        return v == "1" || v.eq_ignore_ascii_case("true");
    }
    true
}
