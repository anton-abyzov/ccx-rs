use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct NotebookEditTool;

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &str {
        "NotebookEdit"
    }

    fn description(&self) -> &str {
        "Edit a cell in a Jupyter notebook (.ipynb file)"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "Absolute path to the .ipynb file"
                },
                "cell_index": {
                    "type": "integer",
                    "description": "Index of the cell to edit (0-based)"
                },
                "new_source": {
                    "type": "string",
                    "description": "New source content for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "description": "Optional: change cell type (code, markdown, raw)",
                    "enum": ["code", "markdown", "raw"]
                }
            },
            "required": ["notebook_path", "cell_index", "new_source"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let notebook_path = input["notebook_path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("notebook_path is required".into()))?;

        let cell_index = input["cell_index"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput("cell_index is required".into()))?
            as usize;

        let new_source = input["new_source"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("new_source is required".into()))?;

        let new_cell_type = input["cell_type"].as_str();

        // Resolve path relative to working directory if not absolute.
        let path = if std::path::Path::new(notebook_path).is_absolute() {
            std::path::PathBuf::from(notebook_path)
        } else {
            ctx.working_dir.join(notebook_path)
        };

        // Read the notebook.
        let content = std::fs::read_to_string(&path).map_err(ToolError::Io)?;

        let mut notebook: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ToolError::Execution(format!("invalid notebook JSON: {e}")))?;

        // Validate it's a notebook.
        if notebook.get("cells").is_none() {
            return Err(ToolError::Execution(
                "not a valid Jupyter notebook (missing 'cells' key)".into(),
            ));
        }

        let cells = notebook["cells"]
            .as_array_mut()
            .ok_or_else(|| ToolError::Execution("'cells' is not an array".into()))?;

        if cell_index >= cells.len() {
            return Err(ToolError::InvalidInput(format!(
                "cell_index {cell_index} out of range (notebook has {} cells)",
                cells.len()
            )));
        }

        // Update the cell source — split into lines as ipynb format expects.
        let source_lines: Vec<serde_json::Value> = new_source
            .lines()
            .enumerate()
            .map(|(i, line)| {
                let total_lines = new_source.lines().count();
                if i < total_lines - 1 {
                    serde_json::Value::String(format!("{line}\n"))
                } else {
                    serde_json::Value::String(line.to_string())
                }
            })
            .collect();

        cells[cell_index]["source"] = serde_json::Value::Array(source_lines);

        // Optionally change cell type.
        if let Some(ct) = new_cell_type {
            cells[cell_index]["cell_type"] = serde_json::Value::String(ct.to_string());
        }

        // Clear outputs for code cells to avoid stale output.
        if cells[cell_index]["cell_type"].as_str() == Some("code") {
            cells[cell_index]["outputs"] = serde_json::Value::Array(Vec::new());
            cells[cell_index]["execution_count"] = serde_json::Value::Null;
        }

        // Extract cell type before dropping mutable borrow.
        let cell_type = cells[cell_index]["cell_type"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        // Write back.
        let output = serde_json::to_string_pretty(&notebook)
            .map_err(|e| ToolError::Execution(format!("failed to serialize: {e}")))?;
        std::fs::write(&path, output).map_err(ToolError::Io)?;

        Ok(ToolResult {
            content: format!(
                "Updated cell {cell_index} ({cell_type}) in {}",
                path.display()
            ),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx(suffix: &str) -> ToolContext {
        let dir = std::env::temp_dir().join(format!("ccx_test_notebook_{suffix}"));
        let _ = std::fs::create_dir_all(&dir);
        ToolContext::new(dir)
    }

    fn sample_notebook() -> serde_json::Value {
        json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {
                "kernelspec": {
                    "display_name": "Python 3",
                    "language": "python",
                    "name": "python3"
                }
            },
            "cells": [
                {
                    "cell_type": "code",
                    "source": ["print('hello')\n"],
                    "metadata": {},
                    "outputs": [],
                    "execution_count": null
                },
                {
                    "cell_type": "markdown",
                    "source": ["# Title\n"],
                    "metadata": {}
                }
            ]
        })
    }

    #[test]
    fn test_notebook_edit_schema() {
        let tool = NotebookEditTool;
        assert_eq!(tool.name(), "NotebookEdit");
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "notebook_path"));
        assert!(required.iter().any(|v| v == "cell_index"));
        assert!(required.iter().any(|v| v == "new_source"));
    }

    #[tokio::test]
    async fn test_notebook_edit_code_cell() {
        let ctx = test_ctx("code");
        let nb_path = ctx.working_dir.join("test.ipynb");
        let nb = sample_notebook();
        std::fs::write(&nb_path, serde_json::to_string_pretty(&nb).unwrap()).unwrap();

        let tool = NotebookEditTool;
        let result = tool
            .execute(
                json!({
                    "notebook_path": nb_path.to_str().unwrap(),
                    "cell_index": 0,
                    "new_source": "print('world')"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("cell 0"));

        // Verify the file was updated.
        let updated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&nb_path).unwrap()).unwrap();
        let source = updated["cells"][0]["source"][0].as_str().unwrap();
        assert!(source.contains("world"));

        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_notebook_edit_markdown_cell() {
        let ctx = test_ctx("md");
        let nb_path = ctx.working_dir.join("test2.ipynb");
        std::fs::write(
            &nb_path,
            serde_json::to_string_pretty(&sample_notebook()).unwrap(),
        )
        .unwrap();

        let tool = NotebookEditTool;
        let result = tool
            .execute(
                json!({
                    "notebook_path": nb_path.to_str().unwrap(),
                    "cell_index": 1,
                    "new_source": "# Updated Title"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("markdown"));

        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_notebook_edit_out_of_range() {
        let ctx = test_ctx("range");
        let nb_path = ctx.working_dir.join("test3.ipynb");
        std::fs::write(
            &nb_path,
            serde_json::to_string_pretty(&sample_notebook()).unwrap(),
        )
        .unwrap();

        let tool = NotebookEditTool;
        let err = tool
            .execute(
                json!({
                    "notebook_path": nb_path.to_str().unwrap(),
                    "cell_index": 99,
                    "new_source": "x"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));

        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_notebook_edit_invalid_file() {
        let ctx = test_ctx("invalid");
        let tool = NotebookEditTool;
        let err = tool
            .execute(
                json!({
                    "notebook_path": "/nonexistent/notebook.ipynb",
                    "cell_index": 0,
                    "new_source": "x"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Io(_)));
    }

    #[tokio::test]
    async fn test_notebook_edit_not_notebook() {
        let ctx = test_ctx("notnb");
        let path = ctx.working_dir.join("not_notebook.ipynb");
        std::fs::write(&path, r#"{"key": "value"}"#).unwrap();

        let tool = NotebookEditTool;
        let err = tool
            .execute(
                json!({
                    "notebook_path": path.to_str().unwrap(),
                    "cell_index": 0,
                    "new_source": "x"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));

        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_notebook_edit_change_cell_type() {
        let ctx = test_ctx("ctype");
        let nb_path = ctx.working_dir.join("test_type.ipynb");
        std::fs::write(
            &nb_path,
            serde_json::to_string_pretty(&sample_notebook()).unwrap(),
        )
        .unwrap();

        let tool = NotebookEditTool;
        let result = tool
            .execute(
                json!({
                    "notebook_path": nb_path.to_str().unwrap(),
                    "cell_index": 0,
                    "new_source": "# Now markdown",
                    "cell_type": "markdown"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);

        let updated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&nb_path).unwrap()).unwrap();
        assert_eq!(updated["cells"][0]["cell_type"].as_str(), Some("markdown"));

        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_notebook_edit_missing_params() {
        let ctx = test_ctx("params");
        let tool = NotebookEditTool;

        let err = tool.execute(json!({}), &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));

        let err = tool
            .execute(json!({"notebook_path": "x"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));

        let err = tool
            .execute(json!({"notebook_path": "x", "cell_index": 0}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_notebook_edit_relative_path() {
        let ctx = test_ctx("relpath");
        let nb_path = ctx.working_dir.join("relative.ipynb");
        std::fs::write(
            &nb_path,
            serde_json::to_string_pretty(&sample_notebook()).unwrap(),
        )
        .unwrap();

        let tool = NotebookEditTool;
        let result = tool
            .execute(
                json!({
                    "notebook_path": "relative.ipynb",
                    "cell_index": 0,
                    "new_source": "x = 1"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }
}
