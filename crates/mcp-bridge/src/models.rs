//! Model discovery: `search_models` in its three modes.
//!
//! Shapes verified live 2026-08-24 -- docs/MCP-SURFACE.md 11.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// One row from `search_models(query=)`.
///
/// Only `name`, `type` and `tags` are modelled: on the local surface the other
/// fields the payload carries (`base_model`, `trained_words`, `source_url`,
/// `preview_url`, `size`, `id`) are always null, and `is_public` always false --
/// they belong to the cloud registry, which this app does not query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHit {
    /// Model file name, or a path within a subdirectory (e.g.
    /// `ACE-Step-v1.5-ambient_dream1-LoRA\adapter_model.safetensors`).
    pub name: String,
    /// Model kind / folder: `diffusion_models`, `loras`, `vae`, ...
    #[serde(rename = "type", default)]
    pub ty: String,
    /// Gallery tags; on this surface just the folder name repeated.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// `search_models(query=)` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSearch {
    /// Total matches across the whole catalog.
    #[serde(default)]
    pub total: usize,
    /// Rows in this response.
    #[serde(default)]
    pub shown: usize,
    /// The matches themselves.
    #[serde(default)]
    pub rows: Vec<ModelHit>,
}

/// One file from `search_models(folder=)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    /// File name.
    pub name: String,
    /// comfy-cli's index into the folder listing.
    #[serde(rename = "pathIndex", default)]
    pub path_index: usize,
}

/// `search_models(folder=)` result.
///
///  Different shape from [`ModelSearch`]: `files`, not `rows`, and each entry
/// is `{name, pathIndex}` rather than `{name, type, tags}`. A wrapper reading
/// `rows` out of a folder result (or vice versa) sees nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFolder {
    /// The folder that was listed.
    #[serde(default)]
    pub folder: Option<String>,
    /// Total files in the folder.
    #[serde(default)]
    pub total: usize,
    /// Files returned.
    #[serde(default)]
    pub shown: usize,
    /// The files themselves.
    #[serde(default)]
    pub files: Vec<ModelFile>,
}

/// One folder from `search_models()` (list-folders).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFolderEntry {
    /// Folder name, e.g. `checkpoints`.
    pub name: String,
    /// Subfolders within it.
    #[serde(default)]
    pub subfolders: Vec<String>,
}

/// `search_models()` (no args) result -- the folder list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFolders {
    /// Number of folders.
    #[serde(default)]
    pub count: usize,
    /// The folders.
    #[serde(default)]
    pub folders: Vec<ModelFolderEntry>,
}

impl LocalComfy {
    /// Search installed models by file name.
    pub async fn search_models(&self, query: &str) -> Result<ModelSearch, ComfyError> {
        let mut args = Map::new();
        args.insert("query".into(), Value::String(query.to_string()));
        self.call("search_models", args).await
    }

    /// List the files in one model folder.
    pub async fn list_models_in(&self, folder: &str) -> Result<ModelFolder, ComfyError> {
        let mut args = Map::new();
        args.insert("folder".into(), Value::String(folder.to_string()));
        self.call("search_models", args).await
    }

    /// List the model folders themselves.
    pub async fn list_model_folders(&self) -> Result<ModelFolders, ComfyError> {
        self.call("search_models", Map::new()).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;
    use crate::models::{ModelFolder, ModelFolders, ModelSearch};

    #[tokio::test]
    async fn test_search_models_sends_query() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "total": 0, "shown": 0, "rows": []
        }))])
        .await;

        let _: ModelSearch = client.search_models("acestep").await.expect("search");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("search_models"));
        assert_eq!(log[0]["arguments"]["query"], json!("acestep"));
        assert!(log[0]["arguments"].get("folder").is_none());
    }

    #[tokio::test]
    async fn test_list_models_in_sends_folder() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "folder": "checkpoints", "total": 0, "shown": 0, "files": []
        }))])
        .await;

        let _: ModelFolder = client.list_models_in("checkpoints").await.expect("folder");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("search_models"));
        assert_eq!(log[0]["arguments"]["folder"], json!("checkpoints"));
        assert!(log[0]["arguments"].get("query").is_none());
    }

    #[tokio::test]
    async fn test_list_model_folders_sends_no_mode_args() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "count": 0, "folders": []
        }))])
        .await;

        let _: ModelFolders = client.list_model_folders().await.expect("folders");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("search_models"));
        assert!(log[0]["arguments"].get("query").is_none());
        assert!(log[0]["arguments"].get("folder").is_none());
    }

    /// Protects: query mode decodes `rows` with `type` (renamed `ty`), and the
    /// always-null cloud-registry fields are simply absent.
    #[tokio::test]
    async fn test_search_decodes_rows_with_type() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "mode": "local",
            "filters": { "text": "acestep", "type": null, "include_public": null },
            "total": 2,
            "shown": 2,
            "rows": [
                { "name": "acestep_v1.5_xl_turbo_bf16.safetensors", "type": "diffusion_models",
                  "tags": ["diffusion_models"], "base_model": null, "trained_words": null,
                  "source_url": null, "preview_url": null, "size": null,
                  "is_public": false, "id": null },
                { "name": "ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors",
                  "type": "loras", "tags": ["loras"], "base_model": null,
                  "trained_words": null, "source_url": null, "preview_url": null,
                  "size": null, "is_public": false, "id": null }
            ]
        }))])
        .await;

        let search: ModelSearch = client.search_models("acestep").await.expect("search");
        assert_eq!(search.total, 2);
        assert_eq!(search.rows.len(), 2);
        assert_eq!(
            search.rows[0].name,
            "acestep_v1.5_xl_turbo_bf16.safetensors"
        );
        assert_eq!(search.rows[0].ty, "diffusion_models");
        assert_eq!(search.rows[1].ty, "loras");
        assert!(search.rows[1].name.contains('\\'));
    }

    /// Protects: folder mode decodes `files` with camelCase `pathIndex` -- the
    /// other half of the "different shapes" trap, and a serde rename.
    #[tokio::test]
    async fn test_folder_decodes_files_with_path_index() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "mode": "local",
            "url": "http://127.0.0.1:8188/models/diffusion_models",
            "folder": "diffusion_models",
            "total": 3,
            "shown": 3,
            "files": [
                { "name": "acestep_v1.5_xl_turbo_bf16.safetensors", "pathIndex": 0 },
                { "name": "minimax_h3_fl2va_pruned_int8_convrot.safetensors", "pathIndex": 1 },
                { "name": "minimax_music3_dit_int8_convrot.safetensors", "pathIndex": 2 }
            ]
        }))])
        .await;

        let folder: ModelFolder = client
            .list_models_in("diffusion_models")
            .await
            .expect("folder");
        assert_eq!(folder.folder.as_deref(), Some("diffusion_models"));
        assert_eq!(folder.files.len(), 3);
        assert_eq!(
            folder.files[2].name,
            "minimax_music3_dit_int8_convrot.safetensors"
        );
        assert_eq!(folder.files[2].path_index, 2);
    }

    #[tokio::test]
    async fn test_folders_decodes_the_folder_list() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "mode": "local",
            "url": "http://127.0.0.1:8188/models",
            "count": 2,
            "folders": [
                { "name": "checkpoints", "subfolders": [] },
                { "name": "loras", "subfolders": ["ACE-Step-v1.5-ambient_dream1-LoRA"] }
            ]
        }))])
        .await;

        let folders: ModelFolders = client.list_model_folders().await.expect("folders");
        assert_eq!(folders.count, 2);
        assert_eq!(folders.folders.len(), 2);
        assert_eq!(folders.folders[1].name, "loras");
        assert_eq!(folders.folders[1].subfolders.len(), 1);
    }
}
