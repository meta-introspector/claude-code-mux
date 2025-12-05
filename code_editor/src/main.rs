use anyhow::{Context, Result};
use clap::Parser;
use quote::quote;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use syn::{
    visit_mut::{visit_item_fn_mut, VisitMut},
    File,
    Item,
    ItemFn,
    ItemUse,
};
use toml;
use proc_macro2;

/// --- Configuration Structures for Edit Jobs ---

#[derive(Debug, Deserialize)]
pub struct EditJobConfig {
    pub edits: Vec<EditJob>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum EditJob {
    AddUse(AddUseDetails),
    RemoveFunction(RemoveFunctionDetails),
    ReplaceExpression(ReplaceExpressionDetails),
    AddFunction(AddFunctionDetails),
    AddItem(AddItemDetails),
    ReplaceFileContent(ReplaceFileContentDetails),
    ReplaceFileContentFromFile(ReplaceFileContentFromFileDetails),
    // Add other edit types as needed
}

#[derive(Debug, Deserialize)]
pub struct AddUseDetails {
    pub target_file: PathBuf,
    pub path: String, // e.g., "use super::error::AppError;"
    pub position: Option<AddUsePosition>,
}

#[derive(Debug, Deserialize)]
pub enum AddUsePosition {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "end")]
    End,
    #[serde(rename = "after_use_path")]
    AfterUsePath(String), // Path of an existing use statement to place after
}

#[derive(Debug, Deserialize)]
pub struct RemoveFunctionDetails {
    pub target_file: PathBuf,
    pub function_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceExpressionDetails {
    pub target_file: PathBuf,
    pub function_name: String, // Function containing the expression
    pub old_code_snippet: String, // The snippet to replace
    pub new_code_snippet: String, // The replacement snippet
}

#[derive(Debug, Deserialize)]
pub struct AddFunctionDetails {
    pub target_file: PathBuf,
    pub function_code: String, // Full code of the function to add
}

// New struct for adding any Item (struct, enum, function, impl, etc.)
#[derive(Debug, Deserialize)]
pub struct AddItemDetails {
    pub target_file: PathBuf,
    pub item_code: String, // Full code of the item to add
}

#[derive(Debug, Deserialize)]
pub struct ReplaceFileContentDetails {
    pub target_file: PathBuf,
    pub new_content: String,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceFileContentFromFileDetails {
    pub target_file: PathBuf,
    pub source_file: PathBuf,
}

/// Command-line arguments for the code editor.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the edit job configuration file or a directory containing edit job files.
    #[arg(short, long, value_name = "PATH")]
    config_path: PathBuf,
}


/// --- Main Logic ---

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config_paths = get_config_paths(&cli.config_path)
        .with_context(|| format!("Failed to get config paths from {:?}", cli.config_path))?;

    if config_paths.is_empty() {
        println!("No edit job files found in {:?}", cli.config_path);
        return Ok(())
    }

    for config_path in config_paths {
        println!("\nStarting code editing process based on {:?}...", config_path);

        let config_content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {:?}", config_path))?;
        let edit_job_config: EditJobConfig = toml::from_str(&config_content)
            .with_context(|| format!("Failed to parse {:?}", config_path))?;

        // Group edits by target file to process each file once
        let mut edits_by_file: HashMap<PathBuf, Vec<&EditJob>> = HashMap::new();
        for edit in &edit_job_config.edits {
            let target_file = match edit {
                EditJob::AddUse(details) => &details.target_file,
                EditJob::RemoveFunction(details) => &details.target_file,
                EditJob::ReplaceExpression(details) => &details.target_file,
                EditJob::AddFunction(details) => &details.target_file,
                EditJob::AddItem(details) => &details.target_file,
                EditJob::ReplaceFileContent(details) => &details.target_file,
                EditJob::ReplaceFileContentFromFile(details) => &details.target_file,
            };
            edits_by_file.entry(target_file.clone()).or_default().push(edit);
        }

        for (file_path, edits) in edits_by_file {
            println!("\nProcessing file: {:?}", file_path);

            let mut replace_entire_file = false;
            let mut new_file_content_from_string: Option<String> = None;
            let mut new_file_content_from_file: Option<PathBuf> = None;

            for edit in &edits {
                match edit {
                    EditJob::ReplaceFileContent(details) => {
                        replace_entire_file = true;
                        new_file_content_from_string = Some(details.new_content.clone());
                        break; // No need to process other edits if we're replacing the entire file
                    }
                    EditJob::ReplaceFileContentFromFile(details) => {
                        replace_entire_file = true;
                        new_file_content_from_file = Some(details.source_file.clone());
                        break; // No need to process other edits if we're replacing the entire file
                    }
                    _ => {} // Other edits don't trigger full file replacement
                }
            }

            if replace_entire_file {
                if let Some(content) = new_file_content_from_string {
                    apply_replace_file_content(&file_path, &content)?;
                    println!("Successfully replaced content of {:?}", file_path);
                } else if let Some(source_path) = new_file_content_from_file {
                    apply_replace_file_content_from_file(&file_path, &source_path)?;
                    println!("Successfully replaced content of {:?} from source file {:?}", file_path, source_path);
                }
            } else {
                let file_content = fs::read_to_string(&file_path)
                    .with_context(|| format!("Failed to read file: {:?}", file_path))?;
                let mut ast: File = syn::parse_file(&file_content)
                    .with_context(|| format!("Failed to parse Rust file: {:?}", file_path))?;

                for edit in &edits {
                    match edit {
                        EditJob::AddUse(details) => apply_add_use(&mut ast, details)?,
                        EditJob::RemoveFunction(details) => apply_remove_function(&mut ast, details)?,
                        EditJob::ReplaceExpression(details) => apply_replace_expression(&mut ast, details)?,
                        EditJob::AddFunction(details) => apply_add_function(&mut ast, details)?,
                        EditJob::AddItem(details) => apply_add_item(&mut ast, details)?,
                        EditJob::ReplaceFileContent(_) => { /* Already handled */ }
                        EditJob::ReplaceFileContentFromFile(_) => { /* Already handled */ }
                    }
                }

                let formatted_code = prettyplease::unparse(&ast);
                fs::write(&file_path, formatted_code)
                    .with_context(|| format!("Failed to write modified file: {:?}", file_path))?;
                println!("Successfully applied edits to {:?}", file_path);
            }
        }
    }

    println!("\nAll specified edits applied successfully!");
    Ok(())
}

/// Helper function to get a list of config files.
/// If path is a file, returns a vector containing just that path.
/// If path is a directory, returns all .toml files within it, sorted by name.
fn get_config_paths(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if path.is_file() {
        paths.push(path.to_path_buf());
    } else if path.is_dir() {
        let mut dir_entries: Vec<_> = fs::read_dir(path)?.filter_map(|entry| {
            let entry = entry.ok()?;
            let entry_path = entry.path();
            if entry_path.is_file() && entry_path.extension().map_or(false, |ext| ext == "toml") {
                Some(entry_path)
            } else {
                None
            }
        }).collect();
        dir_entries.sort(); // Sort by name
        paths.extend(dir_entries);
    } else {
        anyhow::bail!("Path {:?} is neither a file nor a directory.", path);
    }
    Ok(paths)
}

/// --- Edit Application Functions ---

fn apply_add_use(ast: &mut File, details: &AddUseDetails) -> Result<()> {
    let new_use_item: ItemUse = syn::parse_str(&details.path)
        .with_context(|| format!("Invalid use statement syntax: {}", details.path))?;

    let new_use_str = quote! { #new_use_item }.to_string();

    // Check for duplicates
    for item in &ast.items {
        if let Item::Use(existing_use) = item {
            if quote! { #existing_use }.to_string() == new_use_str {
                println!("  Skipping duplicate use statement: {}", details.path);
                return Ok(())
            }
        }
    }

    match &details.position {
        Some(AddUsePosition::Start) => {
            ast.items.insert(0, Item::Use(new_use_item.clone()));
        }
        Some(AddUsePosition::End) | None => {
            // Find the last use statement and insert after it
            let mut last_use_idx = 0;
            for (idx, item) in ast.items.iter().enumerate() {
                if let Item::Use(_) = item {
                    last_use_idx = idx;
                }
            }
            ast.items.insert(last_use_idx + 1, Item::Use(new_use_item.clone()));
        }
        Some(AddUsePosition::AfterUsePath(after_path)) => {
            let after_use_item: ItemUse = syn::parse_str(after_path)
                .with_context(|| format!("Invalid 'after_use_path' syntax: {}", after_path))?;
            let after_use_str = quote! { #after_use_item }.to_string();

            let mut inserted = false;
            for (idx, item) in ast.items.iter().enumerate() {
                if let Item::Use(existing_use) = item {
                    if quote! { #existing_use }.to_string() == after_use_str {
                        ast.items.insert(idx + 1, Item::Use(new_use_item.clone()));
                        inserted = true;
                        break;
                    }
                }
            }
            if !inserted {
                println!("  Warning: 'after_use_path' not found. Inserting at end of uses.");
                ast.items.push(Item::Use(new_use_item));
            }
        }
    }
    println!("  Added use statement: {}", details.path);
    Ok(())
}

fn apply_remove_function(ast: &mut File, details: &RemoveFunctionDetails) -> Result<()> {
    let mut removed = false;
    ast.items.retain(|item| {
        if let Item::Fn(item_fn) = item {
            if item_fn.sig.ident == details.function_name {
                removed = true;
                return false; // Remove this function
            }
        }
        true // Keep other items
    });
    if removed {
        println!("  Removed function: {}", details.function_name);
    } else {
        println!("  Warning: Function '{}' not found for removal.", details.function_name);
    }
    Ok(())
}

fn apply_replace_expression(ast: &mut File, details: &ReplaceExpressionDetails) -> Result<()> {
    let mut visitor = ExpressionReplacer {
        function_name: &details.function_name,
        old_snippet: syn::parse_str(&details.old_code_snippet)
            .context("Failed to parse old code snippet")?,
        new_snippet: syn::parse_str(&details.new_code_snippet)
            .context("Failed to parse new code snippet")?,
        replaced_count: 0,
    };
    visitor.visit_file_mut(ast);

    if visitor.replaced_count > 0 {
        println!(
            "  Replaced {} occurrences of '{}' with '{}' in function '{}'.",
            visitor.replaced_count, details.old_code_snippet, details.new_code_snippet, details.function_name
        );
    } else {
        println!(
            "  Warning: No occurrences of '{}' found in function '{}' for replacement.",
            details.old_code_snippet, details.function_name
        );
    }
    Ok(())
}

struct ExpressionReplacer<'a> {
    function_name: &'a str,
    old_snippet: syn::Expr,
    new_snippet: syn::Expr,
    replaced_count: usize,
}

impl<'a> VisitMut for ExpressionReplacer<'a> {
    fn visit_item_fn_mut(&mut self, i: &mut ItemFn) {
        if i.sig.ident == self.function_name {
            // Found the target function, now visit its block
            self.visit_block_mut(&mut i.block);
        }
        // Continue visiting other functions if any
        visit_item_fn_mut(self, i); // Call default visitor for ItemFn to go deeper
    }

    fn visit_expr_mut(&mut self, i: &mut syn::Expr) {
        use quote::ToTokens;
        let mut i_tokens = proc_macro2::TokenStream::new();
        i.to_tokens(&mut i_tokens);
        let mut old_snippet_tokens = proc_macro2::TokenStream::new();
        self.old_snippet.to_tokens(&mut old_snippet_tokens);

        if i_tokens.to_string() == old_snippet_tokens.to_string() {
            *i = self.new_snippet.clone();
            self.replaced_count += 1;
        }
        // Important: Recurse into children of the expression
        syn::visit_mut::visit_expr_mut(self, i);
    }
}

fn apply_add_function(ast: &mut File, details: &AddFunctionDetails) -> Result<()> {
    let new_fn: ItemFn = syn::parse_str(&details.function_code)
        .with_context(|| format!("Invalid function code syntax: {}", details.function_code))?;

    // Check if a function with the same name already exists
    let new_fn_name = new_fn.sig.ident.to_string();
    for item in &ast.items {
        if let Item::Fn(existing_fn) = item {
            if existing_fn.sig.ident.to_string() == new_fn_name {
                println!("  Warning: Function '{}' already exists. Skipping addition.", new_fn_name);
                return Ok(())
            }
        }
    }

    ast.items.push(Item::Fn(new_fn));
    println!("  Added function: {}", new_fn_name);
    Ok(())
}

fn apply_add_item(ast: &mut File, details: &AddItemDetails) -> Result<()> {
    let new_item: Item = syn::parse_str(&details.item_code)
        .with_context(|| format!("Invalid item code syntax: {}", details.item_code))?;

    // Check for duplicates based on item type and name
    let item_name = match &new_item {
        Item::Const(i) => Some(i.ident.to_string()),
        Item::Enum(i) => Some(i.ident.to_string()),
        Item::Fn(i) => Some(i.sig.ident.to_string()),
        Item::Struct(i) => Some(i.ident.to_string()),
        Item::Macro(i) => i.ident.as_ref().map(|id| id.to_string()),
        Item::Mod(i) => Some(i.ident.to_string()),
        Item::Trait(i) => Some(i.ident.to_string()),
        Item::TraitAlias(i) => Some(i.ident.to_string()),
        Item::Type(i) => Some(i.ident.to_string()),
        Item::Union(i) => Some(i.ident.to_string()),
        Item::Use(_i) => None, // Use statements are handled by apply_add_use with different logic
        _ => None, // For other item types, we might not have a simple name or want to check duplicates
    };

    if let Some(name) = item_name {
        for item in &ast.items {
            let existing_name = match item {
                Item::Const(i) => Some(i.ident.to_string()),
                Item::Enum(i) => Some(i.ident.to_string()),
                Item::Fn(i) => Some(i.sig.ident.to_string()),
                Item::Struct(i) => Some(i.ident.to_string()),
                Item::Macro(i) => i.ident.as_ref().map(|id| id.to_string()),
                Item::Mod(i) => Some(i.ident.to_string()),
                Item::Trait(i) => Some(i.ident.to_string()),
                Item::TraitAlias(i) => Some(i.ident.to_string()),
                Item::Type(i) => Some(i.ident.to_string()),
                Item::Union(i) => Some(i.ident.to_string()),
                _ => None,
            };
            if existing_name.map_or(false, |n| n == name) {
                println!("  Warning: Item '{}' already exists. Skipping addition.", name);
                return Ok(());
            }
        }
    }


    ast.items.push(new_item);
    println!("  Added item to file.");
    Ok(())
}

fn apply_replace_file_content(file_path: &PathBuf, new_content: &str) -> Result<()> {
    fs::write(file_path, new_content)
        .with_context(|| format!("Failed to write modified file: {:?}", file_path))?;
    Ok(())
}

fn apply_replace_file_content_from_file(target_file: &PathBuf, source_file: &PathBuf) -> Result<()> {
    let new_content = fs::read_to_string(source_file)
        .with_context(|| format!("Failed to read source file: {:?}", source_file))?;
    fs::write(target_file, new_content)
        .with_context(|| format!("Failed to write modified file: {:?}", target_file))?;
    Ok(())
}