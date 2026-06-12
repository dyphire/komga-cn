#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryScanInterval {
    Disabled,
    Hourly,
    Every6h,
    Every12h,
    Daily,
    Weekly,
}

impl LibraryScanInterval {
    pub fn duration_seconds(self) -> Option<u64> {
        match self {
            Self::Disabled => None,
            Self::Hourly => Some(60 * 60),
            Self::Every6h => Some(6 * 60 * 60),
            Self::Every12h => Some(12 * 60 * 60),
            Self::Daily => Some(24 * 60 * 60),
            Self::Weekly => Some(7 * 24 * 60 * 60),
        }
    }

    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::Disabled => "DISABLED",
            Self::Hourly => "HOURLY",
            Self::Every6h => "EVERY_6H",
            Self::Every12h => "EVERY_12H",
            Self::Daily => "DAILY",
            Self::Weekly => "WEEKLY",
        }
    }

    pub fn from_persisted_name(value: &str) -> Option<Self> {
        match value {
            "DISABLED" => Some(Self::Disabled),
            "HOURLY" => Some(Self::Hourly),
            "EVERY_6H" => Some(Self::Every6h),
            "EVERY_12H" => Some(Self::Every12h),
            "DAILY" => Some(Self::Daily),
            "WEEKLY" => Some(Self::Weekly),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibrarySeriesCover {
    First,
    FirstUnreadOrFirst,
    FirstUnreadOrLast,
    Last,
}

impl LibrarySeriesCover {
    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::First => "FIRST",
            Self::FirstUnreadOrFirst => "FIRST_UNREAD_OR_FIRST",
            Self::FirstUnreadOrLast => "FIRST_UNREAD_OR_LAST",
            Self::Last => "LAST",
        }
    }

    pub fn from_persisted_name(value: &str) -> Option<Self> {
        match value {
            "FIRST" => Some(Self::First),
            "FIRST_UNREAD_OR_FIRST" => Some(Self::FirstUnreadOrFirst),
            "FIRST_UNREAD_OR_LAST" => Some(Self::FirstUnreadOrLast),
            "LAST" => Some(Self::Last),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryRecord {
    pub id: String,
    pub name: String,
    pub root: String,
    pub import_comicinfo_book: bool,
    pub import_comicinfo_series: bool,
    pub import_comicinfo_collection: bool,
    pub import_comicinfo_readlist: bool,
    pub import_comicinfo_series_append_volume: bool,
    pub import_epub_book: bool,
    pub import_epub_series: bool,
    pub import_mylar_series: bool,
    pub import_local_artwork: bool,
    pub import_barcode_isbn: bool,
    pub scan_force_modified_time: bool,
    pub scan_interval: LibraryScanInterval,
    pub scan_on_startup: bool,
    pub scan_cbx: bool,
    pub scan_pdf: bool,
    pub scan_epub: bool,
    pub scan_directory_exclusions: Vec<String>,
    pub repair_extensions: bool,
    pub convert_to_cbz: bool,
    pub empty_trash_after_scan: bool,
    pub series_cover: LibrarySeriesCover,
    pub hash_files: bool,
    pub hash_pages: bool,
    pub hash_koreader: bool,
    pub analyze_dimensions: bool,
    pub oneshots_directory: Option<String>,
    pub unavailable: bool,
}

impl LibraryRecord {
    pub fn default_record(id: String) -> Self {
        Self {
            id,
            name: String::new(),
            root: String::new(),
            import_comicinfo_book: true,
            import_comicinfo_series: true,
            import_comicinfo_collection: true,
            import_comicinfo_readlist: true,
            import_comicinfo_series_append_volume: true,
            import_epub_book: true,
            import_epub_series: true,
            import_mylar_series: true,
            import_local_artwork: true,
            import_barcode_isbn: true,
            scan_force_modified_time: false,
            scan_interval: LibraryScanInterval::Every6h,
            scan_on_startup: false,
            scan_cbx: true,
            scan_pdf: true,
            scan_epub: true,
            scan_directory_exclusions: vec![],
            repair_extensions: false,
            convert_to_cbz: false,
            empty_trash_after_scan: false,
            series_cover: LibrarySeriesCover::First,
            hash_files: true,
            hash_pages: false,
            hash_koreader: false,
            analyze_dimensions: true,
            oneshots_directory: None,
            unavailable: false,
        }
    }

    pub fn apply_changes(&mut self, changes: LibraryChangeSet) {
        if let Some(name) = changes.name {
            self.name = name;
        }
        if let Some(root) = changes.root {
            self.root = root;
        }
        if let Some(value) = changes.import_comicinfo_book {
            self.import_comicinfo_book = value;
        }
        if let Some(value) = changes.import_comicinfo_series {
            self.import_comicinfo_series = value;
        }
        if let Some(value) = changes.import_comicinfo_collection {
            self.import_comicinfo_collection = value;
        }
        if let Some(value) = changes.import_comicinfo_readlist {
            self.import_comicinfo_readlist = value;
        }
        if let Some(value) = changes.import_comicinfo_series_append_volume {
            self.import_comicinfo_series_append_volume = value;
        }
        if let Some(value) = changes.import_epub_book {
            self.import_epub_book = value;
        }
        if let Some(value) = changes.import_epub_series {
            self.import_epub_series = value;
        }
        if let Some(value) = changes.import_mylar_series {
            self.import_mylar_series = value;
        }
        if let Some(value) = changes.import_local_artwork {
            self.import_local_artwork = value;
        }
        if let Some(value) = changes.import_barcode_isbn {
            self.import_barcode_isbn = value;
        }
        if let Some(value) = changes.scan_force_modified_time {
            self.scan_force_modified_time = value;
        }
        if let Some(value) = changes.scan_interval {
            self.scan_interval = value;
        }
        if let Some(value) = changes.scan_on_startup {
            self.scan_on_startup = value;
        }
        if let Some(value) = changes.scan_cbx {
            self.scan_cbx = value;
        }
        if let Some(value) = changes.scan_pdf {
            self.scan_pdf = value;
        }
        if let Some(value) = changes.scan_epub {
            self.scan_epub = value;
        }
        if let Some(value) = changes.scan_directory_exclusions {
            self.scan_directory_exclusions = value;
        }
        if let Some(value) = changes.repair_extensions {
            self.repair_extensions = value;
        }
        if let Some(value) = changes.convert_to_cbz {
            self.convert_to_cbz = value;
        }
        if let Some(value) = changes.empty_trash_after_scan {
            self.empty_trash_after_scan = value;
        }
        if let Some(value) = changes.series_cover {
            self.series_cover = value;
        }
        if let Some(value) = changes.hash_files {
            self.hash_files = value;
        }
        if let Some(value) = changes.hash_pages {
            self.hash_pages = value;
        }
        if let Some(value) = changes.hash_koreader {
            self.hash_koreader = value;
        }
        if let Some(value) = changes.analyze_dimensions {
            self.analyze_dimensions = value;
        }
        if let Some(value) = changes.oneshots_directory {
            self.oneshots_directory = value;
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibraryChangeSet {
    pub name: Option<String>,
    pub root: Option<String>,
    pub import_comicinfo_book: Option<bool>,
    pub import_comicinfo_series: Option<bool>,
    pub import_comicinfo_collection: Option<bool>,
    pub import_comicinfo_readlist: Option<bool>,
    pub import_comicinfo_series_append_volume: Option<bool>,
    pub import_epub_book: Option<bool>,
    pub import_epub_series: Option<bool>,
    pub import_mylar_series: Option<bool>,
    pub import_local_artwork: Option<bool>,
    pub import_barcode_isbn: Option<bool>,
    pub scan_force_modified_time: Option<bool>,
    pub scan_interval: Option<LibraryScanInterval>,
    pub scan_on_startup: Option<bool>,
    pub scan_cbx: Option<bool>,
    pub scan_pdf: Option<bool>,
    pub scan_epub: Option<bool>,
    pub scan_directory_exclusions: Option<Vec<String>>,
    pub repair_extensions: Option<bool>,
    pub convert_to_cbz: Option<bool>,
    pub empty_trash_after_scan: Option<bool>,
    pub series_cover: Option<LibrarySeriesCover>,
    pub hash_files: Option<bool>,
    pub hash_pages: Option<bool>,
    pub hash_koreader: Option<bool>,
    pub analyze_dimensions: Option<bool>,
    pub oneshots_directory: Option<Option<String>>,
}
