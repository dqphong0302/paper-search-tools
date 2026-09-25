//! Interest library, project workspaces and search history.

use super::*;

#[derive(serde::Deserialize)]
pub(super) struct WorkspacePaperQuery {
    paper_id: String,
}


#[derive(serde::Deserialize)]
pub(super) struct SaveSearchRequest {
    #[serde(default)]
    saved: bool,
}


// Handler 6: Search History (optionally scoped to a workspace)
pub(super) async fn get_search_history_handler(
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> Json<Vec<SearchHistoryItem>> {
    let history = state
        .db
        .get_search_history(100, params.workspace_id.as_deref());
    Json(history)
}

// ---- Interest library ----------------------------------------------------

pub(super) async fn list_library_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.library_papers() {
        Ok(papers) => (
            StatusCode::OK,
            Json(serde_json::to_value(papers).unwrap_or_default()),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn add_library_paper_handler(
    State(state): State<AppState>,
    Json(payload): Json<WorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.add_library_paper(&payload.paper) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn update_library_paper_handler(
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
    Json(payload): Json<UpdateWorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.update_library_paper(
        &params.paper_id,
        payload.note.as_deref(),
        payload.status.as_deref(),
        payload.favorite,
        payload.tags.as_deref(),
    ) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn remove_library_paper_handler(
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.remove_library_paper(&params.paper_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

// ---- Legacy workspaces (kept for existing MCP clients) ------------------

pub(super) async fn list_workspaces_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Json<Vec<Workspace>> {
    let mut workspaces = state.db.list_workspaces();
    if let Ok(crate::agents::Principal::Agent(grant)) =
        crate::agents::authenticate(&state.db, &headers)
    {
        workspaces.retain(|w| crate::agents::workspace_allowed(&grant, &w.id, false));
    }
    Json(workspaces)
}

pub(super) async fn create_workspace_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateWorkspaceRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .db
        .create_workspace(&payload.name, payload.description.as_deref())
    {
        Ok(workspace) => (
            StatusCode::OK,
            Json(serde_json::to_value(workspace).unwrap()),
        ),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn update_workspace_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<UpdateWorkspaceRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(existing) = state
        .db
        .list_workspaces()
        .into_iter()
        .find(|workspace| workspace.id == id)
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Workspace not found" })),
        );
    };
    let name = payload.name.unwrap_or(existing.name);
    let description = payload.description.or(existing.description);
    match state
        .db
        .rename_workspace(&id, &name, description.as_deref())
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn delete_workspace_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.delete_workspace(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn list_workspace_papers_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !state.db.workspace_exists(&id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Workspace not found" })),
        );
    }
    match state.db.workspace_papers(&id) {
        Ok(papers) => (StatusCode::OK, Json(serde_json::to_value(papers).unwrap())),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn add_workspace_paper_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<WorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .db
        .add_workspace_paper(&id, &payload.paper, payload.note.as_deref())
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn update_workspace_note_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
    Json(payload): Json<UpdateWorkspacePaperRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.update_workspace_paper(
        &id,
        &params.paper_id,
        payload.note.as_deref(),
        payload.status.as_deref(),
        payload.favorite,
        payload.tags.as_deref(),
    ) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub(super) async fn save_search_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<SaveSearchRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.set_search_saved(&id, payload.saved) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

pub(super) async fn remove_workspace_paper_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(params): Query<WorkspacePaperQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.remove_workspace_paper(&id, &params.paper_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}


pub(super) async fn clear_search_history_handler(
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .db
        .clear_search_history(params.workspace_id.as_deref())
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

pub(super) async fn delete_search_history_item_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.delete_search_history_item(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}
