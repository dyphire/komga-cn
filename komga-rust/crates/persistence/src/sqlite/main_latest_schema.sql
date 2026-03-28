CREATE TABLE ANNOUNCEMENTS_READ
(
    USER_ID         varchar NOT NULL,
    ANNOUNCEMENT_ID varchar NOT NULL,
    PRIMARY KEY (USER_ID, ANNOUNCEMENT_ID),
    FOREIGN KEY (USER_ID) references USER (ID)
);

CREATE TABLE AUTHENTICATION_ACTIVITY
(
    USER_ID    varchar  NULL     DEFAULT NULL,
    EMAIL      varchar  NULL     DEFAULT NULL,
    IP         varchar  NULL     DEFAULT NULL,
    USER_AGENT varchar  NULL     DEFAULT NULL,
    SUCCESS    boolean  NOT NULL,
    ERROR      varchar  NULL     DEFAULT NULL,
    DATE_TIME       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    SOURCE          varchar  NULL     DEFAULT NULL,
    API_KEY_ID      varchar  NULL     DEFAULT NULL,
    API_KEY_COMMENT varchar  NULL     DEFAULT NULL,
    FOREIGN KEY (USER_ID) references USER (ID)
);

CREATE TABLE BOOK
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FILE_LAST_MODIFIED datetime NOT NULL,
    NAME               varchar  NOT NULL,
    URL                varchar  NOT NULL,
    SERIES_ID          varchar  NOT NULL,
    FILE_SIZE          int8     NOT NULL DEFAULT 0,
    NUMBER             int      NOT NULL DEFAULT 0,
    LIBRARY_ID         varchar  NOT NULL,
    FILE_HASH          varchar  NOT NULL DEFAULT '',
    DELETED_DATE       datetime NULL     DEFAULT NULL,
    oneshot            boolean  NOT NULL DEFAULT 0,
    FILE_HASH_KOREADER varchar  NOT NULL DEFAULT '',
    FOREIGN KEY (LIBRARY_ID) REFERENCES LIBRARY (ID),
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE BOOK_METADATA
(
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    NUMBER             varchar  NOT NULL,
    NUMBER_LOCK        boolean  NOT NULL DEFAULT 0,
    NUMBER_SORT        real     NOT NULL,
    NUMBER_SORT_LOCK   boolean  NOT NULL DEFAULT 0,
    RELEASE_DATE       date     NULL,
    RELEASE_DATE_LOCK  boolean  NOT NULL DEFAULT 0,
    SUMMARY            varchar  NOT NULL DEFAULT '',
    SUMMARY_LOCK       boolean  NOT NULL DEFAULT 0,
    TITLE              varchar  NOT NULL,
    TITLE_LOCK         boolean  NOT NULL DEFAULT 0,
    AUTHORS_LOCK       boolean  NOT NULL DEFAULT 0,
    TAGS_LOCK          boolean  NOT NULL DEFAULT 0,
    BOOK_ID            varchar  NOT NULL PRIMARY KEY,
    ISBN               varchar  NOT NULL DEFAULT '',
    ISBN_LOCK          boolean  NOT NULL DEFAULT 0,
    LINKS_LOCK         boolean  NOT NULL DEFAULT 0,
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE BOOK_METADATA_AGGREGATION
(
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    RELEASE_DATE       date     NULL,
    SUMMARY            varchar  NOT NULL DEFAULT '',
    SUMMARY_NUMBER     varchar  NOT NULL DEFAULT '',
    SERIES_ID          varchar  NOT NULL PRIMARY KEY,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE BOOK_METADATA_AGGREGATION_AUTHOR
(
    NAME      varchar NOT NULL,
    ROLE      varchar NOT NULL,
    SERIES_ID varchar NOT NULL,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE BOOK_METADATA_AGGREGATION_TAG
(
    TAG       varchar NOT NULL,
    SERIES_ID varchar NOT NULL,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE BOOK_METADATA_AUTHOR
(
    NAME    varchar NOT NULL,
    ROLE    varchar NOT NULL,
    BOOK_ID varchar NOT NULL,
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE BOOK_METADATA_LINK
(
    LABEL   varchar NOT NULL,
    URL     varchar NOT NULL,
    BOOK_ID varchar NOT NULL,
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE BOOK_METADATA_TAG
(
    TAG     varchar NOT NULL,
    BOOK_ID varchar NOT NULL,
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE CLIENT_SETTINGS_GLOBAL
(
    KEY                varchar NOT NULL PRIMARY KEY,
    VALUE              varchar NOT NULL,
    ALLOW_UNAUTHORIZED boolean NOT NULL default 0
);

CREATE TABLE CLIENT_SETTINGS_USER
(
    USER_ID varchar NOT NULL,
    KEY     varchar NOT NULL,
    VALUE   varchar NOT NULL,
    FOREIGN KEY (USER_ID) REFERENCES USER (ID),
    PRIMARY KEY (KEY, USER_ID)
);

CREATE TABLE COLLECTION
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    NAME               varchar  NOT NULL,
    ORDERED            boolean  NOT NULL DEFAULT 0,
    SERIES_COUNT       int      NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE COLLECTION_SERIES
(
    COLLECTION_ID varchar NOT NULL,
    SERIES_ID     varchar NOT NULL,
    NUMBER        int     NOT NULL,
    PRIMARY KEY (COLLECTION_ID, SERIES_ID),
    FOREIGN KEY (COLLECTION_ID) REFERENCES COLLECTION (ID),
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE HISTORICAL_EVENT
(
    ID        varchar PRIMARY KEY,
    TYPE      varchar  NOT NULL,
    BOOK_ID   varchar  NULL,
    SERIES_ID varchar  NULL,
    TIMESTAMP datetime NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE HISTORICAL_EVENT_PROPERTIES
(
    ID    varchar NOT NULL,
    KEY   varchar NOT NULL,
    VALUE varchar NOT NULL,
    PRIMARY KEY (ID, KEY),
    FOREIGN KEY (ID) REFERENCES HISTORICAL_EVENT (ID)
);

CREATE TABLE LIBRARY
(
    ID                          varchar  NOT NULL PRIMARY KEY,
    CREATED_DATE                datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE          datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    NAME                        varchar  NOT NULL,
    ROOT                        varchar  NOT NULL,
    IMPORT_COMICINFO_BOOK       boolean  NOT NULL DEFAULT 1,
    IMPORT_COMICINFO_SERIES     boolean  NOT NULL DEFAULT 1,
    IMPORT_COMICINFO_COLLECTION boolean  NOT NULL DEFAULT 1,
    IMPORT_EPUB_BOOK            boolean  NOT NULL DEFAULT 1,
    IMPORT_EPUB_SERIES          boolean  NOT NULL DEFAULT 1,
    SCAN_FORCE_MODIFIED_TIME    boolean  NOT NULL DEFAULT 0,
    SCAN_STARTUP                boolean  NOT NULL DEFAULT 0,
    IMPORT_LOCAL_ARTWORK        boolean  NOT NULL DEFAULT 1,
    IMPORT_COMICINFO_READLIST   boolean  NOT NULL DEFAULT 1,
    IMPORT_BARCODE_ISBN         boolean  NOT NULL DEFAULT 1,
    CONVERT_TO_CBZ              boolean  NOT NULL DEFAULT 0,
    REPAIR_EXTENSIONS           boolean  NOT NULL DEFAULT 0,
    EMPTY_TRASH_AFTER_SCAN      boolean  NOT NULL DEFAULT 0,
    IMPORT_MYLAR_SERIES         boolean  NOT NULL DEFAULT 1,
    SERIES_COVER                varchar  NOT NULL DEFAULT 'FIRST',
    UNAVAILABLE_DATE            datetime NULL     DEFAULT NULL,
    HASH_FILES                  boolean  NOT NULL DEFAULT 1,
    HASH_PAGES                  boolean  NOT NULL DEFAULT 0,
    ANALYZE_DIMENSIONS          boolean  NOT NULL DEFAULT 1,
    IMPORT_COMICINFO_SERIES_APPEND_VOLUME boolean NOT NULL DEFAULT 1,
    ONESHOTS_DIRECTORY          varchar  NULL     DEFAULT NULL,
    SCAN_CBX                    boolean  NOT NULL DEFAULT 1,
    SCAN_PDF                    boolean  NOT NULL DEFAULT 1,
    SCAN_EPUB                   boolean  NOT NULL DEFAULT 1,
    SCAN_INTERVAL               varchar  NOT NULL DEFAULT 'EVERY_6H',
    HASH_KOREADER               boolean  NOT NULL DEFAULT 0
);

CREATE TABLE LIBRARY_EXCLUSIONS
(
    LIBRARY_ID varchar NOT NULL,
    EXCLUSION  varchar NOT NULL,
    PRIMARY KEY (LIBRARY_ID, EXCLUSION),
    FOREIGN KEY (LIBRARY_ID) REFERENCES LIBRARY (ID)
);

CREATE TABLE MEDIA
(
    MEDIA_TYPE         varchar  NULL,
    STATUS             varchar  NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    COMMENT            varchar  NULL,
    BOOK_ID            varchar  NOT NULL PRIMARY KEY,
    PAGE_COUNT             int      NOT NULL DEFAULT 0,
    EXTENSION_CLASS        varchar  NULL,
    _UNUSED                varchar  NULL,
    EXTENSION_VALUE_BLOB   blob     NULL,
    EPUB_DIVINA_COMPATIBLE boolean  NOT NULL DEFAULT 0,
    EPUB_IS_KEPUB          boolean  NOT NULL DEFAULT 0,
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE MEDIA_FILE
(
    FILE_NAME varchar NOT NULL,
    BOOK_ID   varchar NOT NULL, MEDIA_TYPE varchar NULL, SUB_TYPE varchar NULL, FILE_SIZE int8 NULL,
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE MEDIA_PAGE
(
    FILE_NAME  varchar NOT NULL,
    MEDIA_TYPE varchar NOT NULL,
    NUMBER     int     NOT NULL,
    BOOK_ID    varchar NOT NULL,
    width      int     NULL,
    height     int     NULL,
    FILE_HASH  varchar NOT NULL DEFAULT '',
    FILE_SIZE  int8    NULL,
    PRIMARY KEY (BOOK_ID, NUMBER),
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE PAGE_HASH
(
    HASH               varchar  NOT NULL PRIMARY KEY,
    SIZE               int8     NULL,
    ACTION             varchar  NOT NULL,
    DELETE_COUNT       int      NOT NULL default 0,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE PAGE_HASH_THUMBNAIL
(
    HASH      varchar NOT NULL PRIMARY KEY,
    THUMBNAIL blob    NOT NULL
);

CREATE TABLE READLIST
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    NAME               varchar  NOT NULL,
    BOOK_COUNT         int      NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    SUMMARY            varchar  not NULL default '',
    ORDERED            boolean  NOT NULL DEFAULT 1
);

CREATE TABLE READLIST_BOOK
(
    READLIST_ID varchar NOT NULL,
    BOOK_ID     varchar NOT NULL,
    NUMBER      int     NOT NULL,
    PRIMARY KEY (READLIST_ID, BOOK_ID),
    FOREIGN KEY (READLIST_ID) REFERENCES READLIST (ID),
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE READ_PROGRESS
(
    BOOK_ID            varchar  NOT NULL,
    USER_ID            varchar  NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PAGE               int      NOT NULL,
    COMPLETED          boolean  NOT NULL,
    READ_DATE          datetime NULL     DEFAULT CURRENT_TIMESTAMP,
    device_id          varchar           default '',
    device_name        varchar           default '',
    locator            blob              null,
    PRIMARY KEY (BOOK_ID, USER_ID),
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID),
    FOREIGN KEY (USER_ID) REFERENCES USER (ID)
);

CREATE TABLE READ_PROGRESS_SERIES
(
    SERIES_ID         varchar NOT NULL,
    USER_ID           varchar NOT NULL,
    READ_COUNT        int     NOT NULL,
    IN_PROGRESS_COUNT int     NOT NULL, MOST_RECENT_READ_DATE datetime NULL, LAST_MODIFIED_DATE datetime NULL,
    PRIMARY KEY (SERIES_ID, USER_ID),
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID),
    FOREIGN KEY (USER_ID) REFERENCES USER (ID)
);

CREATE TABLE SERIES
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FILE_LAST_MODIFIED datetime NOT NULL,
    NAME               varchar  NOT NULL,
    URL                varchar  NOT NULL,
    LIBRARY_ID         varchar  NOT NULL,
    BOOK_COUNT         int      NOT NULL DEFAULT 0,
    DELETED_DATE       datetime NULL     DEFAULT NULL,
    oneshot            boolean  NOT NULL DEFAULT 0,
    FOREIGN KEY (LIBRARY_ID) REFERENCES LIBRARY (ID)
);

CREATE TABLE SERIES_METADATA
(
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    STATUS             varchar  NOT NULL,
    STATUS_LOCK        boolean  NOT NULL DEFAULT 0,
    TITLE              varchar  NOT NULL,
    TITLE_LOCK         boolean  NOT NULL DEFAULT 0,
    TITLE_SORT         varchar  NOT NULL,
    TITLE_SORT_LOCK    boolean  NOT NULL DEFAULT 0,
    SERIES_ID              varchar  NOT NULL PRIMARY KEY,
    PUBLISHER              varchar  NOT NULL DEFAULT '',
    PUBLISHER_LOCK         boolean  NOT NULL DEFAULT 0,
    READING_DIRECTION      varchar  NULL,
    READING_DIRECTION_LOCK boolean  NOT NULL DEFAULT 0,
    AGE_RATING             int      NULL,
    AGE_RATING_LOCK        boolean  NOT NULL DEFAULT 0,
    SUMMARY                varchar  NOT NULL DEFAULT '',
    SUMMARY_LOCK           boolean  NOT NULL DEFAULT 0,
    LANGUAGE               varchar  NOT NULL DEFAULT '',
    LANGUAGE_LOCK          boolean  NOT NULL DEFAULT 0,
    GENRES_LOCK            boolean  NOT NULL DEFAULT 0,
    TAGS_LOCK              boolean  NOT NULL DEFAULT 0,
    TOTAL_BOOK_COUNT       int      NULL,
    TOTAL_BOOK_COUNT_LOCK  boolean  NOT NULL DEFAULT 0,
    SHARING_LABELS_LOCK    boolean  NOT NULL DEFAULT 0,
    LINKS_LOCK             boolean  NOT NULL DEFAULT 0,
    ALTERNATE_TITLES_LOCK  boolean  NOT NULL DEFAULT 0,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE SERIES_METADATA_ALTERNATE_TITLE
(
    LABEL     varchar NOT NULL,
    TITLE     varchar NOT NULL,
    SERIES_ID varchar NOT NULL,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE SERIES_METADATA_GENRE
(
    GENRE     varchar NOT NULL,
    SERIES_ID varchar NOT NULL,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE SERIES_METADATA_LINK
(
    LABEL     varchar NOT NULL,
    URL       varchar NOT NULL,
    SERIES_ID varchar NOT NULL,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE SERIES_METADATA_SHARING
(
    LABEL     varchar NOT NULL,
    SERIES_ID varchar NOT NULL,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE SERIES_METADATA_TAG
(
    TAG       varchar NOT NULL,
    SERIES_ID varchar NOT NULL,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE SERVER_SETTINGS
(
    KEY   varchar NOT NULL PRIMARY KEY,
    VALUE varchar NULL
);

CREATE TABLE SIDECAR
(
    URL                varchar  NOT NULL PRIMARY KEY,
    PARENT_URL         varchar  NOT NULL,
    LAST_MODIFIED_TIME datetime NOT NULL,
    LIBRARY_ID         varchar  NOT NULL
);

CREATE TABLE SYNC_POINT
(
    ID           varchar  NOT NULL PRIMARY KEY,
    CREATED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    USER_ID      varchar  NOT NULL,
    API_KEY_ID   varchar  NULL,
    FOREIGN KEY (USER_ID) REFERENCES USER (ID)
);

CREATE TABLE SYNC_POINT_BOOK
(
    SYNC_POINT_ID                         varchar  NOT NULL,
    BOOK_ID                               varchar  NOT NULL,
    BOOK_CREATED_DATE                     datetime NOT NULL,
    BOOK_LAST_MODIFIED_DATE               datetime NOT NULL,
    BOOK_FILE_LAST_MODIFIED               datetime NOT NULL,
    BOOK_FILE_SIZE                        int8     NOT NULL,
    BOOK_FILE_HASH                        varchar  NOT NULL,
    BOOK_METADATA_LAST_MODIFIED_DATE      datetime NOT NULL,
    BOOK_READ_PROGRESS_LAST_MODIFIED_DATE datetime NULL,
    SYNCED                                boolean  NOT NULL default false, BOOK_THUMBNAIL_ID varchar NULL,
    PRIMARY KEY (SYNC_POINT_ID, BOOK_ID),
    FOREIGN KEY (SYNC_POINT_ID) REFERENCES SYNC_POINT (ID)
);

CREATE TABLE SYNC_POINT_BOOK_REMOVED_SYNCED
(
    SYNC_POINT_ID varchar NOT NULL,
    BOOK_ID       varchar NOT NULL,
    PRIMARY KEY (SYNC_POINT_ID, BOOK_ID),
    FOREIGN KEY (SYNC_POINT_ID) REFERENCES SYNC_POINT (ID)
);

CREATE TABLE SYNC_POINT_READLIST
(
    SYNC_POINT_ID               varchar  NOT NULL,
    READLIST_ID                 varchar  NOT NULL,
    READLIST_NAME               varchar  NOT NULL,
    READLIST_CREATED_DATE       datetime NOT NULL,
    READLIST_LAST_MODIFIED_DATE datetime NOT NULL,
    SYNCED                      boolean  NOT NULL default false,
    PRIMARY KEY (SYNC_POINT_ID, READLIST_ID),
    FOREIGN KEY (SYNC_POINT_ID) REFERENCES SYNC_POINT (ID)
);

CREATE TABLE SYNC_POINT_READLIST_BOOK
(
    SYNC_POINT_ID varchar NOT NULL,
    READLIST_ID   varchar NOT NULL,
    BOOK_ID       varchar NOT NULL,
    PRIMARY KEY (SYNC_POINT_ID, READLIST_ID, BOOK_ID),
    FOREIGN KEY (SYNC_POINT_ID) REFERENCES SYNC_POINT (ID)
);

CREATE TABLE SYNC_POINT_READLIST_REMOVED_SYNCED
(
    SYNC_POINT_ID varchar NOT NULL,
    READLIST_ID   varchar NOT NULL,
    PRIMARY KEY (SYNC_POINT_ID, READLIST_ID),
    FOREIGN KEY (SYNC_POINT_ID) REFERENCES SYNC_POINT (ID)
);

CREATE TABLE THUMBNAIL_BOOK
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    THUMBNAIL          blob     NULL,
    URL                varchar  NULL,
    SELECTED           boolean  NOT NULL DEFAULT 0,
    TYPE               varchar  NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    BOOK_ID            varchar  NOT NULL,
    WIDTH              int      NOT NULL default 0,
    HEIGHT             int      NOT NULL default 0,
    MEDIA_TYPE         varchar  NOT NULL default '',
    FILE_SIZE          int8     NOT NULL default 0,
    FOREIGN KEY (BOOK_ID) REFERENCES BOOK (ID)
);

CREATE TABLE THUMBNAIL_COLLECTION
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    SELECTED           boolean  NOT NULL DEFAULT 0,
    THUMBNAIL          blob     NOT NULL,
    TYPE               varchar  NOT NULL,
    COLLECTION_ID      varchar  NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    WIDTH              int      NOT NULL default 0,
    HEIGHT             int      NOT NULL default 0,
    MEDIA_TYPE         varchar  NOT NULL default '',
    FILE_SIZE          int8     NOT NULL default 0,
    FOREIGN KEY (COLLECTION_ID) REFERENCES COLLECTION (ID)
);

CREATE TABLE THUMBNAIL_READLIST
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    SELECTED           boolean  NOT NULL DEFAULT 0,
    THUMBNAIL          blob     NOT NULL,
    TYPE               varchar  NOT NULL,
    READLIST_ID        varchar  NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    WIDTH              int      NOT NULL default 0,
    HEIGHT             int      NOT NULL default 0,
    MEDIA_TYPE         varchar  NOT NULL default '',
    FILE_SIZE          int8     NOT NULL default 0,
    FOREIGN KEY (READLIST_ID) REFERENCES READLIST (ID)
);

CREATE TABLE THUMBNAIL_SERIES
(
    ID                  varchar  NOT NULL PRIMARY KEY,
    URL                 varchar  NULL DEFAULT NULL,
    SELECTED            boolean  NOT NULL DEFAULT 0,
    THUMBNAIL           blob  NULL DEFAULT NULL,
    TYPE                varchar not null,
    CREATED_DATE        datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE  datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    SERIES_ID           varchar  NOT NULL,
    WIDTH               int      NOT NULL default 0,
    HEIGHT              int      NOT NULL default 0,
    MEDIA_TYPE          varchar  NOT NULL default '',
    FILE_SIZE           int8     NOT NULL default 0,
    FOREIGN KEY (SERIES_ID) REFERENCES SERIES (ID)
);

CREATE TABLE "USER"
(
    ID                         varchar                            not null
        primary key,
    CREATED_DATE               datetime default CURRENT_TIMESTAMP not null,
    LAST_MODIFIED_DATE         datetime default CURRENT_TIMESTAMP not null,
    EMAIL                      varchar                            not null
        unique,
    PASSWORD                   varchar                            not null,
    SHARED_ALL_LIBRARIES       boolean  default 1                 not null,
    AGE_RESTRICTION            integer,
    AGE_RESTRICTION_ALLOW_ONLY boolean
);

CREATE TABLE USER_API_KEY
(
    ID                 varchar  NOT NULL PRIMARY KEY,
    USER_ID            varchar  NOT NULL,
    CREATED_DATE       datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    API_KEY            varchar  NOT NULL UNIQUE,
    COMMENT            varchar  NOT NULL,
    FOREIGN KEY (USER_ID) REFERENCES USER (ID)
);

CREATE TABLE USER_LIBRARY_SHARING
(
    USER_ID    varchar NOT NULL,
    LIBRARY_ID varchar NOT NULL,
    PRIMARY KEY (USER_ID, LIBRARY_ID),
    FOREIGN KEY (USER_ID) REFERENCES USER (ID),
    FOREIGN KEY (LIBRARY_ID) REFERENCES LIBRARY (ID)
);

CREATE TABLE USER_ROLE
(
    USER_ID varchar NOT NULL,
    ROLE    varchar NOT NULL,
    PRIMARY KEY (USER_ID, ROLE),
    FOREIGN KEY (USER_ID) REFERENCES USER (ID)
);

CREATE TABLE USER_SHARING
(
    LABEL   varchar NOT NULL,
    ALLOW   boolean NOT NULL,
    USER_ID varchar NOT NULL,
    PRIMARY KEY (LABEL, ALLOW, USER_ID),
    FOREIGN KEY (USER_ID) REFERENCES USER (ID)
);

CREATE INDEX idx__authentication_activity__user_id
    on AUTHENTICATION_ACTIVITY (USER_ID);

CREATE INDEX idx__book__created_date    on BOOK (CREATED_DATE);

CREATE INDEX idx__book__library_id on BOOK (LIBRARY_ID);

CREATE INDEX idx__book__series_id on BOOK (SERIES_ID);

CREATE INDEX idx__book_metadata__number_sort
    on BOOK_METADATA (NUMBER_SORT);

CREATE INDEX idx__book_metadata__release_date
    on BOOK_METADATA (RELEASE_DATE);

CREATE INDEX idx__book_metadata_aggregation_author__series_id on BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID);

CREATE INDEX idx__book_metadata_aggregation_tag__series_id
    on BOOK_METADATA_AGGREGATION_TAG (SERIES_ID);

CREATE INDEX idx__book_metadata_author__book_id on BOOK_METADATA_AUTHOR (BOOK_ID);

CREATE INDEX idx__book_metadata_link__book_id
    on BOOK_METADATA_LINK (BOOK_ID);

CREATE INDEX idx__book_metadata_tag__book_id on BOOK_METADATA_TAG (BOOK_ID);

CREATE INDEX idx__library_exclusions__library_id on LIBRARY_EXCLUSIONS (LIBRARY_ID);

CREATE INDEX idx__media__status
    on MEDIA (STATUS);

CREATE INDEX idx__media_file__book_id on MEDIA_FILE (BOOK_ID);

CREATE INDEX idx__read_progress__last_modified_date
    on READ_PROGRESS (LAST_MODIFIED_DATE);

CREATE INDEX idx__series__created_date
    on SERIES (CREATED_DATE);

CREATE INDEX idx__series__last_modified_date
    on SERIES (LAST_MODIFIED_DATE);

CREATE INDEX idx__series__library_id on SERIES (LIBRARY_ID);

CREATE INDEX idx__series_metadata__title
    on SERIES_METADATA (TITLE);

CREATE INDEX idx__series_metadata_alternate_title__series_id
    on SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID);

CREATE INDEX idx__series_metadata_genre__series_id on SERIES_METADATA_GENRE (SERIES_ID);

CREATE INDEX idx__series_metadata_link__series_id
    on SERIES_METADATA_LINK (SERIES_ID);

CREATE INDEX idx__series_metadata_sharing__series_id
    on SERIES_METADATA_SHARING (SERIES_ID);

CREATE INDEX idx__series_metadata_tag__series_id on SERIES_METADATA_TAG (SERIES_ID);

CREATE INDEX idx__sync_point__user_id
    on SYNC_POINT (USER_ID);

CREATE INDEX idx__sync_point_book__sync_point_id
    on SYNC_POINT_BOOK (SYNC_POINT_ID);

CREATE INDEX idx__sync_point_book_removed_status__sync_point_id
    on SYNC_POINT_BOOK_REMOVED_SYNCED (SYNC_POINT_ID);

CREATE INDEX idx__sync_point_readlist__sync_point_id
    on SYNC_POINT_READLIST (SYNC_POINT_ID);

CREATE INDEX idx__sync_point_readlist_book__sync_point_id_readlist_id
    on SYNC_POINT_READLIST_BOOK (SYNC_POINT_ID, READLIST_ID);

CREATE INDEX idx__thumbnail_book__book_id on THUMBNAIL_BOOK (BOOK_ID);

CREATE INDEX idx__thumbnail_book__file_size
    on THUMBNAIL_BOOK (FILE_SIZE);

CREATE INDEX idx__thumbnail_book__height
    on THUMBNAIL_BOOK (HEIGHT);

CREATE INDEX idx__thumbnail_book__width
    on THUMBNAIL_BOOK (WIDTH);

CREATE INDEX idx__thumbnail_collection__collection_id
    on THUMBNAIL_COLLECTION (COLLECTION_ID);

CREATE INDEX idx__thumbnail_readlist__readlist_id
    on THUMBNAIL_READLIST (READLIST_ID);

CREATE INDEX idx__thumbnail_series__series_id
    on THUMBNAIL_SERIES (SERIES_ID);

CREATE INDEX idx__user_api_key__user_id
    on USER_API_KEY (USER_ID);
