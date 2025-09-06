use color_eyre::Result;
use color_eyre::eyre::eyre;
use envx_core::wizard::SetupWizard;
use envx_core::{ProjectTemplate, get_builtin_templates};

/// Runs the project setup wizard or applies a specific template.
///
/// # Errors
///
/// Returns an error if:
/// - The specified template is not found
/// - The template setup fails
/// - The interactive wizard encounters an error
pub fn run_wizard(template: Option<String>) -> Result<()> {
    if let Some(template_name) = template {
        // Use template directly
        run_template_setup(&template_name)?;
        Ok(())
    } else {
        // Run interactive wizard
        let mut wizard = SetupWizard::new();
        wizard.run()?;
        Ok(())
    }
}

fn run_template_setup(template_name: &str) -> Result<()> {
    let templates = get_builtin_templates();

    let template = templates
        .iter()
        .find(|t| t.name.to_lowercase() == template_name.to_lowercase())
        .ok_or_else(|| eyre!("Template '{}' not found", template_name))?;

    println!("🚀 Setting up {} project...", template.name);
    println!("{}\n", template.description);

    // Apply the template
    apply_template(template)?;

    println!("\n✅ Project setup complete!");
    Ok(())
}

fn apply_template(template: &ProjectTemplate) -> Result<()> {
    let _ = template;
    // Implementation would create the project structure based on template
    unimplemented!()
}

/// Lists all available project templates.
///
/// # Errors
///
/// This function currently does not return errors, but the `Result` type
/// is used for consistency with the error handling pattern.
pub fn list_templates() -> Result<()> {
    let templates = get_builtin_templates();

    println!("Available project templates:\n");

    for template in templates {
        println!("  {} - {}", template.name, template.description);
    }

    println!("\nUse: envx init --template <name>");
    Ok(())
}
