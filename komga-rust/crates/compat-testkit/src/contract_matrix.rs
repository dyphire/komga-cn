#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceFamily {
    pub name: &'static str,
    pub test_targets: &'static [&'static str],
    pub todo_markers: &'static [&'static str],
}

const REQUIRED_SURFACE_FAMILIES: &[SurfaceFamily] = &[
    SurfaceFamily {
        name: "startup/config",
        test_targets: &["runtime_startup_contract"],
        todo_markers: &["startup", "config", "config-dir"],
    },
    SurfaceFamily {
        name: "schema/bootstrap",
        test_targets: &["runtime_schema_contract"],
        todo_markers: &["schema", "bootstrap", "migration", "flyway"],
    },
    SurfaceFamily {
        name: "settings",
        test_targets: &["settings_persistence_contract"],
        todo_markers: &["settings", "server_settings"],
    },
    SurfaceFamily {
        name: "auth/session",
        test_targets: &["auth_session_contract"],
        todo_markers: &[
            "auth",
            "session",
            "oauth",
            "api key",
            "api-key",
            "placeholder_auth",
            "/api/v2/users",
            "/api/v1/claim",
        ],
    },
    SurfaceFamily {
        name: "tasks/scanner",
        test_targets: &["task_runtime_contract", "scanner_persistence_contract"],
        todo_markers: &["task", "scanner", "tasks.sqlite"],
    },
    SurfaceFamily {
        name: "SSE",
        test_targets: &["sse_contract"],
        todo_markers: &["sse"],
    },
    SurfaceFamily {
        name: "libraries",
        test_targets: &["libraries_contract"],
        todo_markers: &["librar"],
    },
    SurfaceFamily {
        name: "series",
        test_targets: &["series_contract"],
        todo_markers: &["series", "oneshot"],
    },
    SurfaceFamily {
        name: "books/media",
        test_targets: &["books_media_contract"],
        todo_markers: &[
            "book",
            "media",
            "thumbnail",
            "read-progress",
            "progression",
            "pdf",
            "page",
            "file",
        ],
    },
    SurfaceFamily {
        name: "readlists/collections",
        test_targets: &["readlists_collections_contract"],
        todo_markers: &["readlist", "collection", "tachiyomi", "comicrack"],
    },
    SurfaceFamily {
        name: "referential",
        test_targets: &["referential_contract"],
        todo_markers: &[
            "referential",
            "authors",
            "genres",
            "languages",
            "publishers",
            "tags",
        ],
    },
    SurfaceFamily {
        name: "support/admin",
        test_targets: &["support_admin_contract"],
        todo_markers: &[
            "控制器对应能力",
            "announcement",
            "release",
            "filesystem",
            "fonts",
            "history",
            "syncpoint",
            "page-hashes",
            "transient-books",
            "actuator",
            "admin",
        ],
    },
    SurfaceFamily {
        name: "OPDS",
        test_targets: &["opds_contract"],
        todo_markers: &["opds"],
    },
    SurfaceFamily {
        name: "device sync",
        test_targets: &["device_sync_contract"],
        todo_markers: &["kobo", "koreader", "device"],
    },
    SurfaceFamily {
        name: "search/WebUI",
        test_targets: &["search_webui_contract"],
        todo_markers: &["search", "webui"],
    },
    SurfaceFamily {
        name: "release cutover",
        test_targets: &["release_cutover_contract"],
        todo_markers: &["full_cutover_contract", "cutover", "替代", "snapshot_json"],
    },
];

const REQUIRED_FULL_CUTOVER_TARGETS: &[&str] = &[
    "runtime_startup_contract",
    "runtime_schema_contract",
    "settings_persistence_contract",
    "auth_session_contract",
    "task_runtime_contract",
    "scanner_persistence_contract",
    "sse_contract",
    "libraries_contract",
    "series_contract",
    "books_media_contract",
    "readlists_collections_contract",
    "referential_contract",
    "support_admin_contract",
    "opds_contract",
    "device_sync_contract",
    "search_webui_contract",
    "release_cutover_contract",
    "full_cutover_contract",
];

pub fn required_surface_families() -> &'static [SurfaceFamily] {
    REQUIRED_SURFACE_FAMILIES
}

pub fn required_full_cutover_targets() -> &'static [&'static str] {
    REQUIRED_FULL_CUTOVER_TARGETS
}

pub fn classify_todo_gap(line: &str) -> Option<&'static str> {
    let lowered = line.to_ascii_lowercase();
    REQUIRED_SURFACE_FAMILIES
        .iter()
        .find(|family| {
            family
                .todo_markers
                .iter()
                .any(|marker| lowered.contains(&marker.to_ascii_lowercase()))
        })
        .map(|family| family.name)
}

pub fn assert_required_target_declared(family_name: &str, test_target: &str) {
    let exists = REQUIRED_SURFACE_FAMILIES
        .iter()
        .any(|family| family.name == family_name && family.test_targets.contains(&test_target));
    assert!(
        exists,
        "missing required contract target mapping: family={family_name}, test_target={test_target}",
    );
}
