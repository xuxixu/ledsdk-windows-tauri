#ifndef LEDSDK_H_INCLUDED
#define LEDSDK_H_INCLUDED

/* Public C ABI for the LED SDK. No C++ or platform header is required. */
#include <stdint.h>

#define LEDSDK_STRUCT_VERSION 1u

#if defined(LEDSDK_BUILD)
#    define LEDSDK_API __declspec(dllexport)
#else
#    define LEDSDK_API __declspec(dllimport)
#endif

#define LEDSDK_CALL __cdecl


#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------
 * Result codes and stable constants
 * ------------------------------------------------------------------------- */
typedef int32_t ledsdk_result_t;

#define LEDSDK_RESULT_OK              ((ledsdk_result_t)0)
#define LEDSDK_RESULT_IN_PROGRESS     ((ledsdk_result_t)1)
#define LEDSDK_RESULT_CANCELLED       ((ledsdk_result_t)2)
#define LEDSDK_RESULT_INVALID_ARGUMENT ((ledsdk_result_t)-1)
#define LEDSDK_RESULT_INVALID_HANDLE  ((ledsdk_result_t)-2)
#define LEDSDK_RESULT_NOT_INITIALIZED ((ledsdk_result_t)-3)
#define LEDSDK_RESULT_INVALID_STATE   ((ledsdk_result_t)-4)
#define LEDSDK_RESULT_ALREADY_STARTED ((ledsdk_result_t)-5)
#define LEDSDK_RESULT_ALREADY_STOPPED ((ledsdk_result_t)-6)
#define LEDSDK_RESULT_TIMEOUT         ((ledsdk_result_t)-7)
#define LEDSDK_RESULT_NOT_FOUND       ((ledsdk_result_t)-8)
#define LEDSDK_RESULT_BUSY            ((ledsdk_result_t)-9)
#define LEDSDK_RESULT_QUEUE_FULL      ((ledsdk_result_t)-10)
#define LEDSDK_RESULT_NOT_SUPPORTED   ((ledsdk_result_t)-11)
#define LEDSDK_RESULT_IO_ERROR        ((ledsdk_result_t)-12)
#define LEDSDK_RESULT_NETWORK_ERROR   ((ledsdk_result_t)-13)
#define LEDSDK_RESULT_PROTOCOL_ERROR  ((ledsdk_result_t)-14)
#define LEDSDK_RESULT_TRANSCODE_ERROR ((ledsdk_result_t)-15)
#define LEDSDK_RESULT_AUTH_FAILED     ((ledsdk_result_t)-16)
#define LEDSDK_RESULT_BUFFER_TOO_SMALL ((ledsdk_result_t)-17)
#define LEDSDK_RESULT_OUT_OF_MEMORY   ((ledsdk_result_t)-18)
#define LEDSDK_RESULT_INTERNAL_ERROR  ((ledsdk_result_t)-100)

#define LEDSDK_TASK_TYPE_SEARCH  1u
#define LEDSDK_TASK_TYPE_EXPORT  2u
#define LEDSDK_TASK_TYPE_SEND    3u
#define LEDSDK_TASK_TYPE_COMMAND 4u
#define LEDSDK_TASK_TYPE_SCREENSHOT 5u
#define LEDSDK_TASK_TYPE_UPGRADE 6u

#define LEDSDK_SERVICE_HTTP   0x00000001u
#define LEDSDK_SERVICE_SEARCH 0x00000002u
#define LEDSDK_SERVICE_EXPORT 0x00000004u
#define LEDSDK_SERVICE_SEND   0x00000008u

#define LEDSDK_DEVICE_FLAG_ONLINE  0x00000001u
#define LEDSDK_DEVICE_FLAG_OFFLINE 0x00000002u

#define LEDSDK_TASK_STATE_QUEUED    0u
#define LEDSDK_TASK_STATE_RUNNING   1u
#define LEDSDK_TASK_STATE_SUCCEEDED 2u
#define LEDSDK_TASK_STATE_FAILED    3u
#define LEDSDK_TASK_STATE_CANCELLED 4u

#define LEDSDK_LOG_ERROR 0u
#define LEDSDK_LOG_WARN  1u
#define LEDSDK_LOG_INFO  2u
#define LEDSDK_LOG_DEBUG 3u

#define LEDSDK_HTTP_FLAG_ALLOW_RANGE       0x00000001u
#define LEDSDK_HTTP_FLAG_ALLOW_OVERWRITE   0x00000002u
#define LEDSDK_HTTP_FLAG_ENABLE_AUTH       0x00000004u
/* Allows GET/HEAD to resolve an explicit Windows drive path such as /d:/input.txt. */
#define LEDSDK_HTTP_FLAG_ALLOW_ABSOLUTE_PATH 0x00000008u

#define LEDSDK_SCREENSHOT_FLAG_OVERWRITE 0x00000001u

#define LEDSDK_FILE_TYPE_LIST          0x00u
#define LEDSDK_FILE_TYPE_AUTHORIZATION 0x01u
#define LEDSDK_FILE_TYPE_PROGRAM       0x02u
#define LEDSDK_FILE_TYPE_UPGRADE       0x03u
#define LEDSDK_FILE_TYPE_SCREEN        0x04u
#define LEDSDK_FILE_TYPE_IMAGE         0x05u
#define LEDSDK_FILE_TYPE_VIDEO         0x06u
#define LEDSDK_FILE_TYPE_AUDIO         0x07u
#define LEDSDK_FILE_TYPE_PLAYLIST      0x08u
#define LEDSDK_FILE_TYPE_GIF           0x09u
#define LEDSDK_FILE_TYPE_UPGRADE_COLOR 0x0au
#define LEDSDK_FILE_TYPE_DIRECT        0x0eu

#define LEDSDK_MEDIA_PICTURE         1u
#define LEDSDK_MEDIA_GIF             2u
#define LEDSDK_MEDIA_VIDEO           3u
#define LEDSDK_MEDIA_AUDIO           4u
#define LEDSDK_MEDIA_SINGLE_LINE_TEXT 5u
#define LEDSDK_MEDIA_MULTI_LINE_TEXT  6u
#define LEDSDK_MEDIA_MIMETIC_CLOCK   7u
#define LEDSDK_MEDIA_DIGITAL_CLOCK   8u
#define LEDSDK_MEDIA_WORD            9u
#define LEDSDK_MEDIA_EXCEL           10u
#define LEDSDK_MEDIA_PPT             11u
#define LEDSDK_MEDIA_COLOR_TEXT      12u
#define LEDSDK_MEDIA_PICTURE_CLOCK   13u
#define LEDSDK_MEDIA_TABLE           14u
#define LEDSDK_MEDIA_ANIMATION_TEXT  15u
#define LEDSDK_MEDIA_BACKGROUND      16u
#define LEDSDK_MEDIA_BORDER          17u
#define LEDSDK_MEDIA_TIMER           18u

/* Display rotation used by playlist/program display attributes. */
#define LEDSDK_ROTATION_0   0u
#define LEDSDK_ROTATION_90  90u
#define LEDSDK_ROTATION_180 180u
#define LEDSDK_ROTATION_270 270u

/* Program week mask: Monday is bit 0, matching the GUI project model. */
#define LEDSDK_WEEK_MONDAY    0x01u
#define LEDSDK_WEEK_TUESDAY   0x02u
#define LEDSDK_WEEK_WEDNESDAY 0x04u
#define LEDSDK_WEEK_THURSDAY  0x08u
#define LEDSDK_WEEK_FRIDAY    0x10u
#define LEDSDK_WEEK_SATURDAY  0x20u
#define LEDSDK_WEEK_SUNDAY    0x40u
#define LEDSDK_WEEK_ALL       0x7fu

/* Common entrance/exit effect values from EMediaMoveMode. */
#define LEDSDK_EFFECT_STATIC           0u
#define LEDSDK_EFFECT_MOVE_LEFT        3u
#define LEDSDK_EFFECT_MOVE_RIGHT       4u
#define LEDSDK_EFFECT_MOVE_UP          5u
#define LEDSDK_EFFECT_MOVE_DOWN        6u
#define LEDSDK_EFFECT_CONTINUOUS_LEFT  7u
#define LEDSDK_EFFECT_CONTINUOUS_RIGHT 8u
#define LEDSDK_EFFECT_CONTINUOUS_UP    9u
#define LEDSDK_EFFECT_CONTINUOUS_DOWN  10u
#define LEDSDK_EFFECT_COVER_BOTTOM     11u
#define LEDSDK_EFFECT_GRADIENT_FADE    12u
#define LEDSDK_EFFECT_BLINDS_HORIZONTAL 13u
#define LEDSDK_EFFECT_BLINDS_VERTICAL   14u

/* Text effects use the common values but expose text-specific names. */
#define LEDSDK_TEXT_EFFECT_STATIC           LEDSDK_EFFECT_STATIC
#define LEDSDK_TEXT_EFFECT_MOVE_LEFT        LEDSDK_EFFECT_MOVE_LEFT
#define LEDSDK_TEXT_EFFECT_MOVE_RIGHT       LEDSDK_EFFECT_MOVE_RIGHT
#define LEDSDK_TEXT_EFFECT_MOVE_UP          LEDSDK_EFFECT_MOVE_UP
#define LEDSDK_TEXT_EFFECT_MOVE_DOWN        LEDSDK_EFFECT_MOVE_DOWN
#define LEDSDK_TEXT_EFFECT_CONTINUOUS_LEFT  LEDSDK_EFFECT_CONTINUOUS_LEFT
#define LEDSDK_TEXT_EFFECT_CONTINUOUS_RIGHT LEDSDK_EFFECT_CONTINUOUS_RIGHT
#define LEDSDK_TEXT_EFFECT_CONTINUOUS_UP    LEDSDK_EFFECT_CONTINUOUS_UP
#define LEDSDK_TEXT_EFFECT_CONTINUOUS_DOWN  LEDSDK_EFFECT_CONTINUOUS_DOWN

/* Value 12 means end-to-end only for a picture exit effect. */
#define LEDSDK_PICTURE_EFFECT_END_TO_END 12u

/* Text alignment values can be ORed for multi-line text. */
#define LEDSDK_TEXT_ALIGN_LEFT     0x01u
#define LEDSDK_TEXT_ALIGN_HCENTER  0x02u
#define LEDSDK_TEXT_ALIGN_RIGHT    0x04u
#define LEDSDK_TEXT_ALIGN_TOP      0x10u
#define LEDSDK_TEXT_ALIGN_VCENTER  0x20u
#define LEDSDK_TEXT_ALIGN_BOTTOM   0x40u

/* Text color modes exposed by TextMaterialWidget. */
#define LEDSDK_TEXT_COLOR_SINGLE                   0u
#define LEDSDK_TEXT_COLOR_HORIZONTAL_LEFT_TO_RIGHT 4u
#define LEDSDK_TEXT_COLOR_HORIZONTAL_RIGHT_TO_LEFT 5u
#define LEDSDK_TEXT_COLOR_VERTICAL_TOP_TO_BOTTOM    9u
#define LEDSDK_TEXT_COLOR_VERTICAL_BOTTOM_TO_TOP    10u
#define LEDSDK_TEXT_COLOR_SLANT                     15u
#define LEDSDK_TEXT_COLOR_ANTI_SLANT                20u
#define LEDSDK_TEXT_COLOR_STATIC_COLORFUL           25u

#define LEDSDK_PLAY_MODE_LOOP    0u
#define LEDSDK_PLAY_MODE_TIME    1u
#define LEDSDK_PLAY_MODE_COUNT   2u
/* Names matching ProgramModelColumn: times, duration, and schedule. */
#define LEDSDK_PLAY_MODE_TIMES     0u
#define LEDSDK_PLAY_MODE_DURATION  1u
#define LEDSDK_PLAY_MODE_SCHEDULE  2u

#define LEDSDK_DEVICE_COMMAND_GET_INFO       1u
#define LEDSDK_DEVICE_COMMAND_REBOOT         2u
#define LEDSDK_DEVICE_COMMAND_RESTORE_FACTORY 3u
/* Backward-compatible name for restore-factory. */
#define LEDSDK_DEVICE_COMMAND_RESET LEDSDK_DEVICE_COMMAND_RESTORE_FACTORY
#define LEDSDK_DEVICE_COMMAND_POWER_ON       4u
#define LEDSDK_DEVICE_COMMAND_POWER_OFF      5u
#define LEDSDK_DEVICE_COMMAND_CLEAR_PROGRAMS 7u

#define LEDSDK_NETCARD_ETHERNET 0u
#define LEDSDK_NETCARD_USB      1u

#define LEDSDK_WIFI_MODE_AP      0u
#define LEDSDK_WIFI_MODE_STATION 1u

#define LEDSDK_LIGHTNESS_MODE_MANUAL 0u
#define LEDSDK_LIGHTNESS_MODE_TIMER  1u
#define LEDSDK_LIGHTNESS_MODE_SENSOR 2u

#define LEDSDK_DEVICE_TEST_MODE_PLAY  0x00u
#define LEDSDK_DEVICE_TEST_MODE_INFO  0x01u
#define LEDSDK_DEVICE_TEST_MODE_TIME  0x02u
#define LEDSDK_DEVICE_TEST_MODE_BLACK 0x10u
#define LEDSDK_DEVICE_TEST_MODE_WHITE 0x11u
#define LEDSDK_DEVICE_TEST_MODE_RED   0x12u
#define LEDSDK_DEVICE_TEST_MODE_GREEN 0x13u
#define LEDSDK_DEVICE_TEST_MODE_BLUE  0x14u

#define LEDSDK_DEVICE_POWER_OFF 0u
#define LEDSDK_DEVICE_POWER_ON  1u

#define LEDSDK_VOLUME_ADJUST_INCREASE 0u
#define LEDSDK_VOLUME_ADJUST_DECREASE 1u

#define LEDSDK_PROGRAM_CONTROL_NEXT     0u
#define LEDSDK_PROGRAM_CONTROL_PREVIOUS 1u
#define LEDSDK_PROGRAM_CONTROL_PLAY     2u
#define LEDSDK_PROGRAM_CONTROL_PAUSE    3u
/* Backward-compatible names for the original previous/next API. */
#define LEDSDK_PROGRAM_SWITCH_NEXT     LEDSDK_PROGRAM_CONTROL_NEXT
#define LEDSDK_PROGRAM_SWITCH_PREVIOUS LEDSDK_PROGRAM_CONTROL_PREVIOUS

#define LEDSDK_INPUT_SOURCE_HDMI1 0u
#define LEDSDK_INPUT_SOURCE_HDMI2 1u
#define LEDSDK_INPUT_SOURCE_VGA   2u
#define LEDSDK_INPUT_SOURCE_BOX   5u

#define LEDSDK_DEVICE_INFO_DISK_VOLUME        0x00000001u
#define LEDSDK_DEVICE_INFO_DISK_FREE_VOLUME   0x00000002u
#define LEDSDK_DEVICE_INFO_SOFTWARE_VERSION   0x00000004u
#define LEDSDK_DEVICE_INFO_HARDWARE_VERSION   0x00000008u
#define LEDSDK_DEVICE_INFO_SYSTEM_VERSION     0x00000010u
#define LEDSDK_DEVICE_INFO_FPGA_VERSION       0x00000020u
#define LEDSDK_DEVICE_INFO_ETHERNET_IP        0x00000040u
#define LEDSDK_DEVICE_INFO_ETHERNET_MASK      0x00000080u
#define LEDSDK_DEVICE_INFO_ETHERNET_GATEWAY   0x00000100u
#define LEDSDK_DEVICE_INFO_ETHERNET_MAC       0x00000200u
#define LEDSDK_DEVICE_INFO_CLOUD_USER_NAME    0x00000400u
#define LEDSDK_DEVICE_INFO_VP_VERSION         0x00000800u
#define LEDSDK_DEVICE_INFO_WIFI_SSID          0x00001000u
#define LEDSDK_DEVICE_INFO_WIFI_IP            0x00002000u
#define LEDSDK_DEVICE_INFO_WIFI_MASK          0x00004000u
#define LEDSDK_DEVICE_INFO_WIFI_GATEWAY       0x00008000u
#define LEDSDK_DEVICE_INFO_WIFI_MAC           0x00010000u
#define LEDSDK_DEVICE_INFO_USB_IP              0x00020000u
#define LEDSDK_DEVICE_INFO_USB_MASK            0x00040000u
#define LEDSDK_DEVICE_INFO_USB_GATEWAY         0x00080000u
#define LEDSDK_DEVICE_INFO_USB_MAC             0x00100000u

/* -------------------------------------------------------------------------
 * Opaque handles and callback/event types
 * ------------------------------------------------------------------------- */
/* Opaque handles keep implementation classes and ownership inside the DLL. */
typedef struct ledsdk_context_s  *ledsdk_context_t;
typedef struct ledsdk_task_s     *ledsdk_task_t;
typedef struct ledsdk_http_s     *ledsdk_http_t;
typedef struct ledsdk_search_s   *ledsdk_search_t;
typedef struct ledsdk_playlist_s *ledsdk_playlist_t;
typedef struct ledsdk_program_s  *ledsdk_program_t;
typedef struct ledsdk_region_s   *ledsdk_region_t;
typedef struct ledsdk_media_s    *ledsdk_media_t;

typedef struct ledsdk_task_event_s {
    uint32_t size;
    uint32_t version;
    uint32_t type;
    uint32_t state;
    uint32_t progress;
    ledsdk_result_t result;
    uint64_t task_id;
    const char *message_utf8;
    const void *data;
    uint32_t data_size;
    uint32_t reserved[2];
} ledsdk_task_event_t;

typedef struct ledsdk_task_status_s {
    uint32_t size;
    uint32_t version;
    uint32_t type;
    uint32_t state;
    uint32_t progress;
    ledsdk_result_t result;
    uint64_t task_id;
    uint32_t reserved[2];
} ledsdk_task_status_t;

typedef struct ledsdk_runtime_status_s {
    uint32_t size;
    uint32_t version;
    uint32_t running_services;
    uint32_t active_task_count;
    uint32_t search_task_count;
    uint32_t export_task_count;
    uint32_t send_task_count;
    uint32_t command_task_count;
    uint32_t reserved[4];
} ledsdk_runtime_status_t;

/* Callbacks execute without SDK internal locks; event data is callback-scoped. */
typedef void (LEDSDK_CALL *ledsdk_log_callback_t)(
    uint32_t level, const char *message_utf8, void *user_data);
typedef void (LEDSDK_CALL *ledsdk_task_callback_t)(
    ledsdk_task_t task, const ledsdk_task_event_t *event, void *user_data);

/* -------------------------------------------------------------------------
 * Configuration and operation option structures
 * ------------------------------------------------------------------------- */
typedef struct ledsdk_config_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_config_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t worker_count;               /* Requested worker count; retained for future use. */
    uint32_t flags;                      /* Reserved configuration flags; set to zero. */
    ledsdk_log_callback_t log_callback;  /* Optional SDK log callback; NULL disables log callbacks. */
    void *log_user_data;                 /* Caller pointer passed unchanged to log_callback. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_config_t;

typedef struct ledsdk_http_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_http_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    const char *bind_address_utf8;       /* Local bind IP; NULL selects 0.0.0.0. Copied by create. */
    uint32_t port;                       /* TCP listen port, 1-65535; zero selects 5860. */
    const char *download_root_utf8;      /* Existing directory served by GET/HEAD; required at start. */
    const char *upload_root_utf8;        /* Existing directory receiving POST/PUT files; required at start. */
    uint64_t max_upload_size;            /* Maximum upload body size in bytes; zero means unlimited. */
    uint32_t request_timeout_ms;         /* Per-request timeout in milliseconds; zero uses libevent default. */
    uint32_t flags;                      /* Bitwise LEDSDK_HTTP_FLAG_* options. */
    const char *auth_token_utf8;         /* Bearer token when authentication is enabled; copied by create. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_http_options_t;

typedef struct ledsdk_http_status_s {
    uint32_t size;                       /* Structure size in bytes, written by the SDK. */
    uint32_t version;                    /* Structure version, written by the SDK. */
    uint32_t running;                    /* Nonzero while the HTTP event loop is running. */
    uint32_t port;                       /* Configured TCP listen port. */
    uint32_t reserved[4];                /* Reserved for ABI extension; ignore on output. */
} ledsdk_http_status_t;

typedef struct ledsdk_search_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_search_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t timeout_ms;                 /* ABI field only; search continues until ledsdk_search_cancel. */
    uint32_t flags;                      /* Reserved search flags; set to zero. */
    uint32_t broadcast_port;             /* UDP discovery port; zero selects protocol port 40000. */
    uint32_t reserved[3];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_search_options_t;

typedef struct ledsdk_device_info_s {
    uint32_t size;                       /* Structure size in bytes, written by the SDK. */
    uint32_t version;                    /* Structure version, written by the SDK. */
    const char *device_id_utf8;          /* SDK-owned unique device ID; valid only for the current callback/query. */
    const char *name_utf8;               /* SDK-owned device name; valid only for the current callback/query. */
    const char *address_utf8;            /* SDK-owned primary remote IPv4 address, or NULL when unavailable. */
    uint32_t port;                       /* Device TCP command port returned by discovery. */
    uint32_t device_type;                /* Product/device type returned by discovery. */
    uint32_t flags;                      /* LEDSDK_DEVICE_FLAG_ONLINE or LEDSDK_DEVICE_FLAG_OFFLINE. */
    uint64_t last_seen_ms;               /* SDK-process monotonic timestamp of the latest response, in ms. */
    uint32_t online;                     /* Nonzero when the latest response is no more than 15 seconds old. */
    uint32_t address_count;              /* Number of network addresses stored for this unique device ID. */
} ledsdk_device_info_t;

typedef struct ledsdk_device_address_s {
    uint32_t size;                       /* Structure size in bytes, written by the SDK. */
    uint32_t version;                    /* Structure version, written by the SDK. */
    const char *address_utf8;            /* SDK-owned device-side IPv4 address. */
    const char *local_address_utf8;      /* SDK-owned local IPv4 address that received the response. */
    uint32_t port;                       /* Device TCP command port used for send connections. */
    uint32_t flags;                      /* Online/offline state for this address; LEDSDK_DEVICE_FLAG_*. */
    uint64_t last_seen_ms;               /* SDK-process monotonic timestamp of this address, in ms. */
    uint32_t reserved[2];                /* Reserved for ABI extension; ignore on output. */
} ledsdk_device_address_t;

/* Device information is valid only during the callback or until the next query. */
typedef void (LEDSDK_CALL *ledsdk_device_callback_t)(
    ledsdk_search_t search, const ledsdk_device_info_t *device, void *user_data);

typedef struct ledsdk_play_rule_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_play_rule_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t mode;                       /* Playback mode; one LEDSDK_PLAY_MODE_* value. */
    uint32_t count;                      /* Playback count used when mode is times (value 0). */
    uint32_t duration_seconds;           /* Playback duration in seconds when mode is duration (value 1). */
    const char *valid_from_utf8;         /* Inclusive start date in YYYY-MM-DD format; copied by setter. */
    const char *valid_to_utf8;           /* Inclusive end date in YYYY-MM-DD format; copied by setter. */
    const char *period_start_utf8;       /* Daily start time in HH:mm:ss format; copied by setter. */
    const char *period_end_utf8;         /* Daily end time in HH:mm:ss format; copied by setter. */
    uint32_t week_mask;                  /* Bitwise LEDSDK_WEEK_* days, using Monday as bit 0. */
    uint32_t flags;                      /* Schedule enables: bit0=date, bit1=time, bit2=week. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_play_rule_t;

typedef struct ledsdk_background_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_background_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t enabled;                    /* Nonzero enables the background. */
    uint32_t mode;                       /* Legacy background mode; use zero unless otherwise documented. */
    uint32_t color_argb;                 /* Solid background color in 0xAARRGGBB format. */
    const char *source_path_utf8;        /* Optional source file path; copied by the setter. */
    uint32_t index;                      /* Built-in background resource index. */
    uint32_t speed;                      /* Built-in background playback speed value. */
    uint32_t reserved[2];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_background_options_t;

typedef struct ledsdk_border_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_border_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t enabled;                    /* Nonzero enables the border. */
    uint32_t index;                      /* Built-in border resource index. */
    uint32_t mode;                       /* Legacy border effect mode. */
    uint32_t speed;                      /* Border effect playback speed value. */
    uint32_t rotate_angle;               /* Border resource rotation angle in degrees. */
    uint32_t color_argb;                 /* Border color in 0xAARRGGBB format. */
    uint32_t left_size;                  /* Left border width in pixels. */
    uint32_t right_size;                 /* Right border width in pixels. */
    uint32_t top_size;                   /* Top border height in pixels. */
    uint32_t bottom_size;                /* Bottom border height in pixels. */
    const char *source_path_utf8;        /* Optional border source file path; copied by the setter. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_border_options_t;

typedef struct ledsdk_font_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_font_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    const char *family_utf8;             /* Font family name; copied by ledsdk_media_set_text_options. */
    uint32_t point_size;                 /* Font point size. */
    uint32_t bold;                       /* Nonzero enables bold text. */
    uint32_t italic;                     /* Nonzero enables italic text. */
    uint32_t underline;                  /* Nonzero enables underlined text. */
    uint32_t strikeout;                  /* Nonzero enables struck-out text. */
    uint32_t color_argb;                 /* Text color in 0xAARRGGBB format. */
    uint32_t mode_3d;                    /* Legacy 3D style value; current renderer ignores it. */
    uint32_t outline;                    /* Legacy outline value; current renderer ignores it. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_font_options_t;

typedef struct ledsdk_text_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_text_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    const char *html_content_utf8;       /* UTF-8 text/HTML content; copied by the setter. */
    ledsdk_font_options_t font;          /* Font and base text-color options. */
    uint32_t kerning;                    /* Absolute inter-character spacing in pixels. */
    uint32_t reserved_legacy_leading;    /* Removed line-spacing field; must be zero. */
    uint32_t color_mode;                 /* One LEDSDK_TEXT_COLOR_* value allowed by the text type. */
    uint32_t reserved_legacy_transform[3]; /* Removed rotation/X/Y fields; must be zero. */
    uint32_t alignment;                  /* Bitwise LEDSDK_TEXT_ALIGN_* horizontal/vertical values. */
    uint32_t mirror;                     /* Bit 0 mirrors horizontally; bit 1 mirrors vertically. */
    uint32_t reserved[3];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_text_options_t;

typedef struct ledsdk_export_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_export_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    const char *output_root_utf8;        /* Required output directory for .zhl/.zhd/.zhp and resources. */
    uint32_t download_port;              /* HTTP port written into package URLs; zero omits download URLs. */
    uint32_t flags;                      /* Reserved export flags; set to zero. */
    uint64_t usb_capacity;               /* Legacy USB capacity field; currently ignored, set to zero. */
    const char *license_file_utf8;       /* Legacy license path; currently ignored, set to NULL. */
    const char *screen_param_file_utf8;  /* Legacy screen-parameter path; currently ignored, set to NULL. */
    const char *full_color_code_file_utf8; /* Legacy full-color code path; currently ignored, set to NULL. */
    const char *low_grey_code_file_utf8; /* Legacy low-grey code path; currently ignored, set to NULL. */
    const char *transcode_cache_root_utf8; /* Persistent transcode cache root; NULL/empty disables caching. */
    const char *download_address_utf8;   /* Device-reachable host/IP written into package download URLs. */
#if defined(_WIN64) || defined(__LP64__) || defined(__x86_64__) || defined(__aarch64__)
    /* The 64-bit layout ends here; no zero-length array is used. */
#else
    uint32_t reserved[2];                /* 32-bit ABI padding; initialize to zero. */
#endif
} ledsdk_export_options_t;

typedef struct ledsdk_send_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_send_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t file_type;                  /* One LEDSDK_FILE_TYPE_* value; programs normally use LIST. */
    uint32_t retry_count;                /* Number of TCP connection retries after the first attempt. */
    uint32_t retry_backoff_ms;           /* Delay between TCP retries in ms; zero selects 300 ms. */
    uint32_t flags;                      /* Reserved send flags; set to zero. */
    const char *password_utf8;           /* Optional device password, at most 31 UTF-8 bytes. */
    const char *md5_hex_utf8;            /* Required 32-character hexadecimal package MD5. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_send_options_t;

/* Device screenshot output options; the SDK copies every input field. */
typedef struct ledsdk_screenshot_options_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof(ledsdk_screenshot_options_t). */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    const char *output_file_utf8;        /* Requested local path; suffix is replaced by device file suffix. */
    uint32_t flags;                      /* Bitwise LEDSDK_SCREENSHOT_FLAG_* options. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_screenshot_options_t;

typedef struct ledsdk_device_command_request_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof this structure. */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t command;                    /* One supported LEDSDK_DEVICE_COMMAND_* value. */
    const void *data;                    /* Optional command payload; copied before the call returns. */
    uint32_t data_size;                  /* Payload size in bytes; zero means no payload. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_device_command_request_t;

typedef struct ledsdk_device_lock_info_s {
    uint32_t size;                       /* Buffer size in bytes; caller sets sizeof this structure. */
    uint32_t version;                    /* Structure version; caller sets LEDSDK_STRUCT_VERSION. */
    uint32_t enabled;                    /* Nonzero when the device lock is enabled. */
    char password_utf8[32];              /* NUL-terminated device password, at most 31 UTF-8 bytes. */
    uint32_t reserved[4];                /* Reserved for ABI extension; ignore on output. */
} ledsdk_device_lock_info_t;

typedef struct ledsdk_device_lock_config_s {
    uint32_t size;                       /* Structure size in bytes; set to sizeof this structure. */
    uint32_t version;                    /* Structure version; set to LEDSDK_STRUCT_VERSION. */
    uint32_t enabled;                    /* 0 disables the lock; 1 enables it. */
    char new_password_utf8[32];          /* NUL-terminated new password, at most 31 UTF-8 bytes. */
    char old_password_utf8[32];          /* NUL-terminated current password; empty if none is set. */
    uint32_t reserved[4];                /* Reserved for ABI extension; initialize to zero. */
} ledsdk_device_lock_config_t;

typedef struct ledsdk_netcard_info_s {
    uint32_t size;                       /* Buffer/structure size; caller sets sizeof this structure. */
    uint32_t version;                    /* Structure version; caller sets LEDSDK_STRUCT_VERSION. */
    uint32_t enabled;                    /* 0 disables the selected adapter; 1 enables it. */
    uint32_t dhcp_enabled;               /* 0 uses static IPv4 fields; 1 enables DHCP. */
    uint8_t mac[6];                      /* Six raw MAC-address bytes. */
    uint8_t reserved0[2];                /* Alignment bytes; initialize to zero. */
    char ip_utf8[16];                    /* NUL-terminated dotted-decimal IPv4 address. */
    char mask_utf8[16];                  /* NUL-terminated dotted-decimal subnet mask. */
    char gateway_utf8[16];               /* NUL-terminated dotted-decimal default gateway. */
    char dns_utf8[16];                   /* NUL-terminated dotted-decimal DNS server. */
    uint32_t port;                       /* Device TCP command port, 0-65535; normally 20003. */
    uint32_t reserved[4];                /* Reserved for ABI extension; zero on input, ignore on output. */
} ledsdk_netcard_info_t;

typedef ledsdk_netcard_info_t ledsdk_netcard_config_t;

typedef struct ledsdk_wifi_info_s {
    uint32_t size;                       /* Buffer/structure size; caller sets sizeof this structure. */
    uint32_t version;                    /* Structure version; caller sets LEDSDK_STRUCT_VERSION. */
    uint32_t enabled;                    /* 0 disables WiFi; 1 enables it. */
    uint32_t mode;                       /* LEDSDK_WIFI_MODE_AP or LEDSDK_WIFI_MODE_STATION. */
    char ssid_utf8[32];                  /* NUL-terminated SSID, at most 31 UTF-8 bytes. */
    char password_utf8[32];              /* NUL-terminated WiFi password, at most 31 UTF-8 bytes. */
    char ip_utf8[16];                    /* AP IPv4 input, or Station IPv4 returned by the device. */
    char mask_utf8[16];                  /* AP subnet mask; Station set-request protocol placeholder. */
    uint32_t station_setting;            /* Legacy Station-mode 32-bit setting; zero in AP mode. */
    uint32_t reserved[4];                /* Reserved for ABI extension; zero on input, ignore on output. */
} ledsdk_wifi_info_t;

typedef ledsdk_wifi_info_t ledsdk_wifi_config_t;

/* Device lightness mode and raw 4-byte timer entries (hour/minute/value/reserved). */
typedef struct ledsdk_device_lightness_info_s {
    uint32_t size;                       /* Buffer size in bytes; caller sets sizeof this structure. */
    uint32_t version;                    /* Structure version; caller sets LEDSDK_STRUCT_VERSION. */
    uint32_t mode;                       /* One LEDSDK_LIGHTNESS_MODE_* value. */
    uint32_t manual_value;               /* Manual brightness level in the range 0-15. */
    uint32_t timed_data_size;            /* Valid byte count in timed_data; protocol records use four bytes. */
    uint8_t timed_data[32];              /* Up to 8 records: hour, minute, brightness, reserved. */
    uint32_t reserved[4];                /* Reserved for ABI extension; ignore on output. */
} ledsdk_device_lightness_info_t;

/* Power-timer state and legacy variable-length timer records. */
typedef struct ledsdk_device_power_timer_info_s {
    uint32_t size;                       /* Buffer size in bytes; caller sets sizeof this structure. */
    uint32_t version;                    /* Structure version; caller sets LEDSDK_STRUCT_VERSION. */
    uint32_t enabled;                    /* Nonzero when device power timers are enabled. */
    uint32_t timer_count;                /* Number of timer records reported by the device. */
    uint32_t timer_data_size;            /* Valid byte count in timer_data. */
    uint8_t timer_data[40];              /* Raw legacy length-prefixed timer records. */
    uint32_t reserved[4];                /* Reserved for ABI extension; ignore on output. */
} ledsdk_device_power_timer_info_t;

/* Caller-owned result for mask-selected device-information UTF-8 fields. */
typedef struct ledsdk_device_system_info_s {
    uint32_t size;                       /* Buffer size in bytes; caller sets sizeof this structure. */
    uint32_t version;                    /* Structure version; caller sets LEDSDK_STRUCT_VERSION. */
    uint32_t field_mask;                 /* LEDSDK_DEVICE_INFO_* fields actually returned by the device. */
    char disk_volume_utf8[128];          /* NUL-terminated total storage capacity text. */
    char disk_free_volume_utf8[128];     /* NUL-terminated free storage capacity text. */
    char software_version_utf8[128];     /* NUL-terminated application/software version. */
    char hardware_version_utf8[128];     /* NUL-terminated hardware version. */
    char system_version_utf8[128];       /* NUL-terminated operating-system version. */
    char fpga_version_utf8[128];         /* NUL-terminated FPGA version. */
    char ethernet_ip_utf8[128];          /* NUL-terminated Ethernet IPv4 address. */
    char ethernet_mask_utf8[128];        /* NUL-terminated Ethernet subnet mask. */
    char ethernet_gateway_utf8[128];     /* NUL-terminated Ethernet default gateway. */
    char ethernet_mac_utf8[128];         /* NUL-terminated Ethernet MAC-address text. */
    char cloud_user_name_utf8[128];      /* NUL-terminated cloud account/user name. */
    char vp_version_utf8[128];           /* NUL-terminated VP component version. */
    char wifi_ssid_utf8[128];            /* NUL-terminated WiFi SSID. */
    char wifi_ip_utf8[128];              /* NUL-terminated WiFi IPv4 address. */
    char wifi_mask_utf8[128];            /* NUL-terminated WiFi subnet mask. */
    char wifi_gateway_utf8[128];         /* NUL-terminated WiFi default gateway. */
    char wifi_mac_utf8[128];             /* NUL-terminated WiFi MAC-address text. */
    char usb_ip_utf8[128];               /* NUL-terminated USB-network IPv4 address. */
    char usb_mask_utf8[128];             /* NUL-terminated USB-network subnet mask. */
    char usb_gateway_utf8[128];          /* NUL-terminated USB-network default gateway. */
    char usb_mac_utf8[128];              /* NUL-terminated USB-network MAC-address text. */
    uint32_t reserved[4];                /* Reserved for ABI extension; ignore on output. */
} ledsdk_device_system_info_t;

/* -------------------------------------------------------------------------
 * Core lifecycle and diagnostics
 * ------------------------------------------------------------------------- */
/* Returns the compiled SDK and ABI versions. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_get_api_version(
    uint32_t *major, uint32_t *minor, uint32_t *patch, uint32_t *abi_version);

/* Converts a result code to UTF-8 text using a caller-owned buffer. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_result_message(
    ledsdk_result_t result, char *buffer, uint32_t capacity, uint32_t *required_size);

/* Creates a context; config may be NULL and is copied during this call. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_initialize(
    const ledsdk_config_t *config, ledsdk_context_t *out_context);

/* Releases a context after all child services and tasks have stopped. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_shutdown(ledsdk_context_t context);

/* Reads the last context-scoped error into a caller-owned buffer. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_get_last_error(
    ledsdk_context_t context, char *buffer, uint32_t capacity, uint32_t *required_size);

/* Returns running service flags and active task counts for one context. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_get_runtime_status(
    ledsdk_context_t context, ledsdk_runtime_status_t *status);

/* -------------------------------------------------------------------------
 * Task lifecycle and asynchronous operation status
 * ------------------------------------------------------------------------- */
/* Reads the current task state; the task remains owned by the caller. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_task_get_status(
    ledsdk_task_t task, ledsdk_task_status_t *status);

/* Requests cancellation; completion is reported through the task callback. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_task_cancel(ledsdk_task_t task);

/* Cancels all context tasks and returns after their callbacks have stopped. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_task_cancel_all(ledsdk_context_t context);

/* Releases a completed or cancelled task handle. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_task_release(ledsdk_task_t task);

/* -------------------------------------------------------------------------
 * HTTP file service
 * ------------------------------------------------------------------------- */
/* Creates an HTTP file service; options may be NULL for defaults. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_http_create(
    ledsdk_context_t context, const ledsdk_http_options_t *options,
    ledsdk_http_t *out_http);

/* Starts the HTTP event loop and binds the configured address and port. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_http_start(ledsdk_http_t http);

/* Stops the HTTP event loop and waits for its worker thread to exit. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_http_stop(ledsdk_http_t http);

/* Returns the current HTTP running state and bound port. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_http_get_status(
    ledsdk_http_t http, ledsdk_http_status_t *status);

/* Releases a stopped HTTP service handle. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_http_release(ledsdk_http_t http);

/* -------------------------------------------------------------------------
 * Device discovery
 * ------------------------------------------------------------------------- */
/* Creates a device search task and registers optional result callbacks. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_create(
    ledsdk_context_t context, const ledsdk_search_options_t *options,
    ledsdk_device_callback_t device_callback, ledsdk_task_callback_t task_callback,
    void *user_data, ledsdk_search_t *out_search);

/* Starts persistent device discovery; it runs until cancelled or failed. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_start(ledsdk_search_t search);

/* Cancels the active search and reports the terminal task state. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_cancel(ledsdk_search_t search);

/* Returns the number of deduplicated devices in the latest search snapshot. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_get_device_count(
    ledsdk_search_t search, uint32_t *count);

/* Returns one device snapshot; string pointers remain owned by the search handle. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_get_device(
    ledsdk_search_t search, uint32_t index, ledsdk_device_info_t *device);

/* Returns the number of network addresses recorded for one device snapshot. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_get_device_address_count(
    ledsdk_search_t search, uint32_t device_index, uint32_t *count);

/* Returns one network address; string pointers remain owned by the search handle. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_get_device_address(
    ledsdk_search_t search, uint32_t device_index, uint32_t address_index,
    ledsdk_device_address_t *address);

/* Releases a search handle after the active search has stopped. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_search_release(ledsdk_search_t search);

/* -------------------------------------------------------------------------
 * Playlist and program model
 * ------------------------------------------------------------------------- */
/* Creates a playlist that owns the target display geometry and program order. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_playlist_create(
    ledsdk_context_t context, uint32_t width, uint32_t height,
    uint32_t device_type, uint32_t flags, ledsdk_playlist_t *out_playlist);

/* Creates a program owned by the playlist; strings are copied during this call. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_create(
    ledsdk_playlist_t playlist, const char *name_utf8,
    const char *background_color_utf8, ledsdk_program_t *out_program);

/* Adds a program to a playlist; a non-NULL rule is applied to that program. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_playlist_add_program(
    ledsdk_playlist_t playlist, ledsdk_program_t program,
    const ledsdk_play_rule_t *rule, uint32_t *out_order);

/* Configures synchronous playback hours for the playlist. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_playlist_set_sync_window(
    ledsdk_playlist_t playlist, uint32_t enabled, uint32_t start_seconds,
    uint32_t end_seconds);

/* Validates the complete playlist and all attached programs. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_playlist_validate(
    ledsdk_playlist_t playlist, char *message, uint32_t capacity,
    uint32_t *required_size);

/* Sets display-level description, rotation (0/90/180/270) and discount width. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_playlist_set_display_attributes(
    ledsdk_playlist_t playlist, const char *description_utf8,
    uint32_t rotate_angle, uint32_t discount_width);

/* Sets the UTF-8 program name; the SDK copies the input string. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_set_name(
    ledsdk_program_t program, const char *name_utf8);

/* Sets program description, control flags, rotation and discount width. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_set_attributes(
    ledsdk_program_t program, const char *description_utf8,
    uint32_t control_flags, uint32_t rotate_angle, uint32_t discount_width);

/* Applies the program-level background configuration; options are copied. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_set_background(
    ledsdk_program_t program, const ledsdk_background_options_t *options);

/* Applies the program-level border configuration; options are copied. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_set_border(
    ledsdk_program_t program, const ledsdk_border_options_t *options);

/* Sets the program schedule; the playlist add operation may also apply it. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_set_play_rule(
    ledsdk_program_t program, const ledsdk_play_rule_t *rule);

/* Validates the in-memory program before export. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_validate(
    ledsdk_program_t program, char *message, uint32_t capacity, uint32_t *required_size);

/* -------------------------------------------------------------------------
 * Region (program file window) and media (region material)
 * ------------------------------------------------------------------------- */

/* Adds a rectangular region and returns an independently releasable handle. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_add_region(
    ledsdk_program_t program, int32_t x, int32_t y, uint32_t width,
    uint32_t height, uint32_t flags, ledsdk_region_t *out_region);

/* Updates a region's file-window geometry in program coordinates. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_region_set_geometry(
    ledsdk_region_t region, int32_t x, int32_t y, uint32_t width, uint32_t height);

/* Sets the region name, alpha (0 transparent to 255 opaque) and z-order. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_region_set_attributes(
    ledsdk_region_t region, const char *name_utf8, uint32_t alpha, int32_t level);

/* Applies the file-window background configuration; options are copied. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_region_set_background(
    ledsdk_region_t region, const ledsdk_background_options_t *options);

/* Applies the file-window border configuration; options are copied. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_region_set_border(
    ledsdk_region_t region, const ledsdk_border_options_t *options);

/* Adds a media type from a UTF-8 path; generated media may omit the path. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_region_add_media(
    ledsdk_region_t region, uint32_t media_type, const char *source_path_utf8,
    ledsdk_media_t *out_media);

/* Adds a UTF-8 text media to a region. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_region_add_text(
    ledsdk_region_t region, const char *text_utf8, ledsdk_media_t *out_media);

/* Sets common media identity and source/content fields; inputs are copied. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_set_attributes(
    ledsdk_media_t media, const char *name_utf8, const char *source_path_utf8,
    const char *content_utf8);

/* Sets media timing in seconds, repeat count, and end-to-end scrolling mode. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_set_timing(
    ledsdk_media_t media, int32_t duration_seconds, uint32_t repeat_times,
    uint32_t end_to_end);

/* Sets common media entrance/exit effects and speeds. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_set_effects(
    ledsdk_media_t media, uint32_t in_effect, uint32_t in_speed,
    uint32_t out_effect, uint32_t out_speed);

/* Sets common media rotation (0/90/180/270) and aspect-ratio mode. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_set_transform(
    ledsdk_media_t media, uint32_t rotate_angle, uint32_t hv_ratio);

/* Applies text and font properties to a text-compatible media type. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_set_text_options(
    ledsdk_media_t media, const ledsdk_text_options_t *options);

/* Applies implementation-defined JSON configuration to extensible media types. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_set_config_json(
    ledsdk_media_t media, const char *config_json_utf8);

/* Returns the stable SDK media type constant. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_get_type(
    ledsdk_media_t media, uint32_t *media_type);

/* Releases a media handle. The parent region removes it from its ownership list. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_media_release(ledsdk_media_t media);

/* Recursively releases a region and all contained media; source files are untouched. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_region_release(ledsdk_region_t region);

/* Recursively releases a program, regions, and media; source files are untouched. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_release(ledsdk_program_t program);

/* Recursively releases a playlist, programs, regions, and media; files are untouched. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_playlist_release(ledsdk_playlist_t playlist);

/* -------------------------------------------------------------------------
 * Local export and device operations
 * ------------------------------------------------------------------------- */
/* Starts asynchronous local export and optional media transcoding. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_program_export(
    ledsdk_context_t context, ledsdk_program_t program,
    const ledsdk_export_options_t *options, ledsdk_task_callback_t callback,
    void *user_data, ledsdk_task_t *out_task);

/* Starts asynchronous export of a complete playlist. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_playlist_export(
    ledsdk_context_t context, ledsdk_playlist_t playlist,
    const ledsdk_export_options_t *options, ledsdk_task_callback_t callback,
    void *user_data, ledsdk_task_t *out_task);

/* Sends an exported program package to one device; same-device tasks are serialized. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_send_program(
    ledsdk_context_t context, const char *device_id_utf8,
    const char *package_path_utf8, const ledsdk_send_options_t *options,
    ledsdk_task_callback_t callback, void *user_data, ledsdk_task_t *out_task);

/* Sends a temporary program package; request format and queue behavior match ledsdk_send_program. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_send_temporary_program(
    ledsdk_context_t context, const char *device_id_utf8,
    const char *package_path_utf8, const ledsdk_send_options_t *options,
    ledsdk_task_callback_t callback, void *user_data, ledsdk_task_t *out_task);

/* Sends one firmware file to a device; same-device tasks are serialized. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_upgrade(
    ledsdk_context_t context, const char *device_id_utf8,
    const char *firmware_path_utf8, const char *md5_hex_utf8,
    ledsdk_task_callback_t callback, void *user_data, ledsdk_task_t *out_task);

/* Captures the screen; success event data is the NUL-terminated actual UTF-8 path. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_screenshot_capture(
    ledsdk_context_t context, const char *device_id_utf8,
    const ledsdk_screenshot_options_t *options,
    ledsdk_task_callback_t callback, void *user_data, ledsdk_task_t *out_task);

/* Enqueues a device command with an optional versioned binary request payload. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_command(
    ledsdk_context_t context, const char *device_id_utf8,
    const ledsdk_device_command_request_t *request,
    ledsdk_task_callback_t callback, void *user_data, ledsdk_task_t *out_task);

/* Reboots the device and blocks until the queued command reaches a terminal result. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_reboot(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t timeout_ms);

/* Deletes all device programs and blocks until the device confirms the command. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_clear_programs(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t timeout_ms);

/* Restores factory settings and blocks until the queued command terminates. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_restore_factory(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t timeout_ms);

/* Sets one supported device test mode and blocks until the command terminates. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_test_mode_set(
    ledsdk_context_t context, const char *device_id_utf8,
    uint32_t mode, uint32_t timeout_ms);

/* Turns the device display on or off and waits for device confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_power_set(
    ledsdk_context_t context, const char *device_id_utf8,
    uint32_t powered_on, uint32_t timeout_ms);

/* Increases or decreases device volume and waits for device confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_volume_adjust(
    ledsdk_context_t context, const char *device_id_utf8,
    uint32_t direction, uint32_t timeout_ms);

/* Switches to the previous or next program and waits for confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_program_switch(
    ledsdk_context_t context, const char *device_id_utf8,
    uint32_t direction, uint32_t timeout_ms);

/* Controls next, previous, play, or pause and waits for device confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_program_control(
    ledsdk_context_t context, const char *device_id_utf8,
    uint32_t action, uint32_t timeout_ms);

/* Selects BOX, VGA, HDMI1, or HDMI2 input and waits for confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_input_source_set(
    ledsdk_context_t context, const char *device_id_utf8,
    uint32_t source, uint32_t timeout_ms);

/* Sets device local date/time and waits for device confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_datetime_set(
    ledsdk_context_t context, const char *device_id_utf8,
    uint32_t year, uint32_t month, uint32_t day,
    uint32_t hour, uint32_t minute, uint32_t second,
    uint32_t week, uint32_t timeout_ms);

/* Reads mask-selected device information; field_mask=0 requests all fields. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_system_info_get(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t field_mask,
    uint32_t timeout_ms, ledsdk_device_system_info_t *out_info);

/* Reads device-lock state through the device queue and blocks until completion. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_lock_get(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t timeout_ms,
    ledsdk_device_lock_info_t *out_info);

/* Sets device-lock state and returns only after the device confirms the request. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_lock_set(
    ledsdk_context_t context, const char *device_id_utf8,
    const ledsdk_device_lock_config_t *config, uint32_t timeout_ms);

/* Reads Ethernet or USB network parameters through the device queue. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_netcard_get(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t netcard_type,
    uint32_t timeout_ms, ledsdk_netcard_info_t *out_info);

/* Sets Ethernet or USB network parameters and waits for device confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_netcard_set(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t netcard_type,
    const ledsdk_netcard_config_t *config, uint32_t timeout_ms);

/* Reads WiFi parameters through the device queue. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_wifi_get(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t timeout_ms,
    ledsdk_wifi_info_t *out_info);

/* Sets WiFi parameters and waits for device confirmation. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_wifi_set(
    ledsdk_context_t context, const char *device_id_utf8,
    const ledsdk_wifi_config_t *config, uint32_t timeout_ms);

/* Reads device lightness mode and timer entries through the device queue. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_lightness_get(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t timeout_ms,
    ledsdk_device_lightness_info_t *out_info);

/* Reads the device power-timer state and raw timer records. */
LEDSDK_API ledsdk_result_t LEDSDK_CALL ledsdk_device_power_timer_get(
    ledsdk_context_t context, const char *device_id_utf8, uint32_t timeout_ms,
    ledsdk_device_power_timer_info_t *out_info);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* LEDSDK_H_INCLUDED */
