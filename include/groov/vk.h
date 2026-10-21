#ifndef GROOV_VK_H
#define GROOV_VK_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define GROOV_VK_DEFAULT_API_VERSION "5.199"
#define GROOV_VK_DEFAULT_PAGE_SIZE 100U
#define GROOV_VK_MAX_PAGE_SIZE 1000U

typedef enum GroovVkStatus {
    GROOV_VK_STATUS_OK = 0,
    GROOV_VK_STATUS_INVALID_ARGUMENT,
    GROOV_VK_STATUS_OUT_OF_MEMORY,
    GROOV_VK_STATUS_NETWORK,
    GROOV_VK_STATUS_HTTP,
    GROOV_VK_STATUS_API,
    GROOV_VK_STATUS_PARSE
} GroovVkStatus;

typedef struct GroovVkError {
    GroovVkStatus Status;
    long HttpStatus;
    int ApiCode;
    char Message[256];
} GroovVkError;

typedef struct GroovVkTrack {
    long long Id;
    long long OwnerId;
    unsigned int DurationSeconds;
    char *Artist;
    char *Title;
    char *Url;
} GroovVkTrack;

typedef struct GroovVkTrackList {
    GroovVkTrack *Items;
    size_t Length;
    size_t Capacity;
    unsigned int TotalCount;
} GroovVkTrackList;

typedef struct GroovVkClient {
    char *AccessToken;
    char *ApiVersion;
    char *BaseUrl;
    long ConnectTimeoutSeconds;
    long RequestTimeoutSeconds;
} GroovVkClient;

void GroovVkErrorClear(GroovVkError *Error);
const char *GroovVkStatusName(GroovVkStatus Status);

GroovVkStatus GroovVkClientInit(
    GroovVkClient *Client,
    const char *AccessToken,
    const char *ApiVersion,
    GroovVkError *Error
);
void GroovVkClientDestroy(GroovVkClient *Client);

GroovVkStatus GroovVkTrackListInit(GroovVkTrackList *List, GroovVkError *Error);
void GroovVkTrackListDestroy(GroovVkTrackList *List);

GroovVkStatus GroovVkClientListAudio(
    const GroovVkClient *Client,
    unsigned int Offset,
    unsigned int Count,
    GroovVkTrackList *Tracks,
    GroovVkError *Error
);

/* Kept public so transport and parser can be tested independently. */
GroovVkStatus GroovVkParseAudioResponse(
    const char *Json,
    size_t JsonLength,
    GroovVkTrackList *Tracks,
    GroovVkError *Error
);

#ifdef __cplusplus
}
#endif

#endif
