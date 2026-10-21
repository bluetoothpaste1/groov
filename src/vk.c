#include "groov/vk.h"

#include <curl/curl.h>

#include <ctype.h>
#include <errno.h>
#include <limits.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char *GroovDuplicateString(const char *Source)
{
    size_t Length;
    char *Copy;

    if (Source == NULL) {
        return NULL;
    }
    Length = strlen(Source);
    Copy = malloc(Length + 1U);
    if (Copy != NULL) {
        memcpy(Copy, Source, Length + 1U);
    }
    return Copy;
}

static void GroovVkSetError(GroovVkError *Error, GroovVkStatus Status, const char *Format, ...)
{
    va_list Arguments;

    if (Error == NULL) {
        return;
    }
    Error->Status = Status;
    va_start(Arguments, Format);
    (void)vsnprintf(Error->Message, sizeof(Error->Message), Format, Arguments);
    va_end(Arguments);
}

void GroovVkErrorClear(GroovVkError *Error)
{
    if (Error != NULL) {
        Error->Status = GROOV_VK_STATUS_OK;
        Error->HttpStatus = 0L;
        Error->ApiCode = 0;
        Error->Message[0] = '\0';
    }
}

const char *GroovVkStatusName(GroovVkStatus Status)
{
    switch (Status) {
    case GROOV_VK_STATUS_OK: return "ok";
    case GROOV_VK_STATUS_INVALID_ARGUMENT: return "invalid_argument";
    case GROOV_VK_STATUS_OUT_OF_MEMORY: return "out_of_memory";
    case GROOV_VK_STATUS_NETWORK: return "network";
    case GROOV_VK_STATUS_HTTP: return "http";
    case GROOV_VK_STATUS_API: return "api";
    case GROOV_VK_STATUS_PARSE: return "parse";
    default: return "unknown";
    }
}

static void GroovVkTrackDestroy(GroovVkTrack *Track)
{
    if (Track != NULL) {
        free(Track->Artist);
        free(Track->Title);
        free(Track->Url);
        memset(Track, 0, sizeof(*Track));
    }
}

GroovVkStatus GroovVkTrackListInit(GroovVkTrackList *List, GroovVkError *Error)
{
    if (List == NULL) {
        GroovVkSetError(Error, GROOV_VK_STATUS_INVALID_ARGUMENT, "track list is null");
        return GROOV_VK_STATUS_INVALID_ARGUMENT;
    }
    memset(List, 0, sizeof(*List));
    GroovVkErrorClear(Error);
    return GROOV_VK_STATUS_OK;
}

void GroovVkTrackListDestroy(GroovVkTrackList *List)
{
    size_t Index;

    if (List == NULL) {
        return;
    }
    for (Index = 0U; Index < List->Length; ++Index) {
        GroovVkTrackDestroy(&List->Items[Index]);
    }
    free(List->Items);
    memset(List, 0, sizeof(*List));
}

static GroovVkStatus GroovVkTrackListAppend(
    GroovVkTrackList *List,
    GroovVkTrack *Track,
    GroovVkError *Error
)
{
    size_t NewCapacity;
    GroovVkTrack *NewItems;

    if (List->Length == List->Capacity) {
        NewCapacity = (List->Capacity == 0U) ? 32U : List->Capacity * 2U;
        if (NewCapacity < List->Capacity || NewCapacity > SIZE_MAX / sizeof(*List->Items)) {
            GroovVkSetError(Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "track list is too large");
            return GROOV_VK_STATUS_OUT_OF_MEMORY;
        }
        NewItems = realloc(List->Items, NewCapacity * sizeof(*List->Items));
        if (NewItems == NULL) {
            GroovVkSetError(Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot grow track list");
            return GROOV_VK_STATUS_OUT_OF_MEMORY;
        }
        List->Items = NewItems;
        List->Capacity = NewCapacity;
    }
    List->Items[List->Length] = *Track;
    memset(Track, 0, sizeof(*Track));
    ++List->Length;
    return GROOV_VK_STATUS_OK;
}

GroovVkStatus GroovVkClientInit(
    GroovVkClient *Client,
    const char *AccessToken,
    const char *ApiVersion,
    GroovVkError *Error
)
{
    const char *Version = (ApiVersion == NULL || ApiVersion[0] == '\0')
        ? GROOV_VK_DEFAULT_API_VERSION : ApiVersion;

    if (Client == NULL || AccessToken == NULL || AccessToken[0] == '\0') {
        GroovVkSetError(Error, GROOV_VK_STATUS_INVALID_ARGUMENT,
                        "an access token is required");
        return GROOV_VK_STATUS_INVALID_ARGUMENT;
    }
    memset(Client, 0, sizeof(*Client));
    Client->AccessToken = GroovDuplicateString(AccessToken);
    Client->ApiVersion = GroovDuplicateString(Version);
    Client->BaseUrl = GroovDuplicateString("https://api.vk.com/method/audio.get");
    Client->ConnectTimeoutSeconds = 10L;
    Client->RequestTimeoutSeconds = 30L;
    if (Client->AccessToken == NULL || Client->ApiVersion == NULL || Client->BaseUrl == NULL) {
        GroovVkClientDestroy(Client);
        GroovVkSetError(Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot initialize VK client");
        return GROOV_VK_STATUS_OUT_OF_MEMORY;
    }
    GroovVkErrorClear(Error);
    return GROOV_VK_STATUS_OK;
}

void GroovVkClientDestroy(GroovVkClient *Client)
{
    if (Client != NULL) {
        free(Client->AccessToken);
        free(Client->ApiVersion);
        free(Client->BaseUrl);
        memset(Client, 0, sizeof(*Client));
    }
}

typedef struct GroovJsonCursor {
    const char *Data;
    size_t Length;
    size_t Position;
    GroovVkError *Error;
} GroovJsonCursor;

static void GroovJsonError(GroovJsonCursor *Cursor, const char *Message)
{
    GroovVkSetError(Cursor->Error, GROOV_VK_STATUS_PARSE, "%s at byte %zu", Message, Cursor->Position);
}

static void GroovJsonSkipWhitespace(GroovJsonCursor *Cursor)
{
    while (Cursor->Position < Cursor->Length &&
           isspace((unsigned char)Cursor->Data[Cursor->Position]) != 0) {
        ++Cursor->Position;
    }
}

static bool GroovJsonConsume(GroovJsonCursor *Cursor, char Expected)
{
    GroovJsonSkipWhitespace(Cursor);
    if (Cursor->Position >= Cursor->Length || Cursor->Data[Cursor->Position] != Expected) {
        GroovJsonError(Cursor, "unexpected JSON character");
        return false;
    }
    ++Cursor->Position;
    return true;
}

static int GroovJsonHexDigit(char Character)
{
    if (Character >= '0' && Character <= '9') return Character - '0';
    if (Character >= 'a' && Character <= 'f') return Character - 'a' + 10;
    if (Character >= 'A' && Character <= 'F') return Character - 'A' + 10;
    return -1;
}

static bool GroovJsonAppendByte(char **Buffer, size_t *Length, size_t *Capacity, unsigned char Byte)
{
    char *NewBuffer;
    size_t NewCapacity;

    if (*Length + 1U >= *Capacity) {
        NewCapacity = (*Capacity == 0U) ? 32U : *Capacity * 2U;
        if (NewCapacity < *Capacity) {
            return false;
        }
        NewBuffer = realloc(*Buffer, NewCapacity);
        if (NewBuffer == NULL) {
            return false;
        }
        *Buffer = NewBuffer;
        *Capacity = NewCapacity;
    }
    (*Buffer)[*Length] = (char)Byte;
    ++*Length;
    (*Buffer)[*Length] = '\0';
    return true;
}

static bool GroovJsonAppendCodepoint(char **Buffer, size_t *Length, size_t *Capacity, unsigned int Codepoint)
{
    if (Codepoint <= 0x7FU) {
        return GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)Codepoint);
    }
    if (Codepoint <= 0x7FFU) {
        return GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0xC0U | (Codepoint >> 6U))) &&
               GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0x80U | (Codepoint & 0x3FU)));
    }
    if (Codepoint <= 0xFFFFU) {
        return GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0xE0U | (Codepoint >> 12U))) &&
               GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0x80U | ((Codepoint >> 6U) & 0x3FU))) &&
               GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0x80U | (Codepoint & 0x3FU)));
    }
    return GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0xF0U | (Codepoint >> 18U))) &&
           GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0x80U | ((Codepoint >> 12U) & 0x3FU))) &&
           GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0x80U | ((Codepoint >> 6U) & 0x3FU))) &&
           GroovJsonAppendByte(Buffer, Length, Capacity, (unsigned char)(0x80U | (Codepoint & 0x3FU)));
}

static bool GroovJsonReadString(GroovJsonCursor *Cursor, char **Value)
{
    char *Buffer = NULL;
    size_t Length = 0U;
    size_t Capacity = 0U;
    unsigned int Codepoint;
    unsigned int LowSurrogate;
    int Digit;
    size_t Index;
    char Character;

    *Value = NULL;
    if (!GroovJsonConsume(Cursor, '"')) {
        return false;
    }
    while (Cursor->Position < Cursor->Length) {
        Character = Cursor->Data[Cursor->Position++];
        if (Character == '"') {
            if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, 0U)) {
                free(Buffer);
                GroovVkSetError(Cursor->Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot allocate JSON string");
                return false;
            }
            *Value = Buffer;
            return true;
        }
        if ((unsigned char)Character < 0x20U) {
            free(Buffer);
            GroovJsonError(Cursor, "control character in JSON string");
            return false;
        }
        if (Character != '\\') {
            if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, (unsigned char)Character)) {
                free(Buffer);
                GroovVkSetError(Cursor->Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot allocate JSON string");
                return false;
            }
            continue;
        }
        if (Cursor->Position >= Cursor->Length) {
            free(Buffer);
            GroovJsonError(Cursor, "unterminated JSON escape");
            return false;
        }
        Character = Cursor->Data[Cursor->Position++];
        switch (Character) {
        case '"': case '\\': case '/':
            if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, (unsigned char)Character)) goto StringOutOfMemory;
            break;
        case 'b': if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, '\b')) goto StringOutOfMemory; break;
        case 'f': if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, '\f')) goto StringOutOfMemory; break;
        case 'n': if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, '\n')) goto StringOutOfMemory; break;
        case 'r': if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, '\r')) goto StringOutOfMemory; break;
        case 't': if (!GroovJsonAppendByte(&Buffer, &Length, &Capacity, '\t')) goto StringOutOfMemory; break;
        case 'u':
            Codepoint = 0U;
            for (Index = 0U; Index < 4U; ++Index) {
                if (Cursor->Position >= Cursor->Length || (Digit = GroovJsonHexDigit(Cursor->Data[Cursor->Position++])) < 0) {
                    free(Buffer);
                    GroovJsonError(Cursor, "invalid Unicode escape");
                    return false;
                }
                Codepoint = (Codepoint << 4U) | (unsigned int)Digit;
            }
            if (Codepoint >= 0xD800U && Codepoint <= 0xDBFFU) {
                if (Cursor->Position + 5U > Cursor->Length || Cursor->Data[Cursor->Position] != '\\' || Cursor->Data[Cursor->Position + 1U] != 'u') {
                    free(Buffer);
                    GroovJsonError(Cursor, "missing low surrogate");
                    return false;
                }
                Cursor->Position += 2U;
                LowSurrogate = 0U;
                for (Index = 0U; Index < 4U; ++Index) {
                    if ((Digit = GroovJsonHexDigit(Cursor->Data[Cursor->Position++])) < 0) {
                        free(Buffer);
                        GroovJsonError(Cursor, "invalid low surrogate");
                        return false;
                    }
                    LowSurrogate = (LowSurrogate << 4U) | (unsigned int)Digit;
                }
                if (LowSurrogate < 0xDC00U || LowSurrogate > 0xDFFFU) {
                    free(Buffer);
                    GroovJsonError(Cursor, "invalid low surrogate");
                    return false;
                }
                Codepoint = 0x10000U + ((Codepoint - 0xD800U) << 10U) + (LowSurrogate - 0xDC00U);
            } else if (Codepoint >= 0xDC00U && Codepoint <= 0xDFFFU) {
                free(Buffer);
                GroovJsonError(Cursor, "unexpected low surrogate");
                return false;
            }
            if (!GroovJsonAppendCodepoint(&Buffer, &Length, &Capacity, Codepoint)) goto StringOutOfMemory;
            break;
        default:
            free(Buffer);
            GroovJsonError(Cursor, "unknown JSON escape");
            return false;
        }
    }
    free(Buffer);
    GroovJsonError(Cursor, "unterminated JSON string");
    return false;

StringOutOfMemory:
    free(Buffer);
    GroovVkSetError(Cursor->Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot allocate JSON string");
    return false;
}

static bool GroovJsonSkipValue(GroovJsonCursor *Cursor);

static bool GroovJsonSkipContainer(GroovJsonCursor *Cursor, char Opening, char Closing)
{
    bool First = true;

    if (!GroovJsonConsume(Cursor, Opening)) return false;
    GroovJsonSkipWhitespace(Cursor);
    if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == Closing) {
        ++Cursor->Position;
        return true;
    }
    while (Cursor->Position < Cursor->Length) {
        if (!First && !GroovJsonConsume(Cursor, ',')) return false;
        First = false;
        if (Opening == '{') {
            char *Key = NULL;
            if (!GroovJsonReadString(Cursor, &Key)) return false;
            free(Key);
            if (!GroovJsonConsume(Cursor, ':')) return false;
        }
        if (!GroovJsonSkipValue(Cursor)) return false;
        GroovJsonSkipWhitespace(Cursor);
        if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == Closing) {
            ++Cursor->Position;
            return true;
        }
    }
    GroovJsonError(Cursor, "unterminated JSON container");
    return false;
}

static bool GroovJsonSkipValue(GroovJsonCursor *Cursor)
{
    char Character;
    char *Ignored = NULL;
    size_t Start;

    GroovJsonSkipWhitespace(Cursor);
    if (Cursor->Position >= Cursor->Length) {
        GroovJsonError(Cursor, "missing JSON value");
        return false;
    }
    Character = Cursor->Data[Cursor->Position];
    if (Character == '"') return GroovJsonReadString(Cursor, &Ignored) && (free(Ignored), true);
    if (Character == '{') return GroovJsonSkipContainer(Cursor, '{', '}');
    if (Character == '[') return GroovJsonSkipContainer(Cursor, '[', ']');
    Start = Cursor->Position;
    while (Cursor->Position < Cursor->Length && strchr(" \t\r\n,]}", Cursor->Data[Cursor->Position]) == NULL) {
        ++Cursor->Position;
    }
    if (Cursor->Position == Start) {
        GroovJsonError(Cursor, "invalid JSON value");
        return false;
    }
    return true;
}

static bool GroovJsonReadInteger(GroovJsonCursor *Cursor, long long *Value)
{
    size_t Start;
    size_t Length;
    char *Number;
    char *End;
    long long Parsed;

    GroovJsonSkipWhitespace(Cursor);
    Start = Cursor->Position;
    if (Cursor->Position < Cursor->Length && (Cursor->Data[Cursor->Position] == '-' || Cursor->Data[Cursor->Position] == '+')) ++Cursor->Position;
    while (Cursor->Position < Cursor->Length && isdigit((unsigned char)Cursor->Data[Cursor->Position]) != 0) ++Cursor->Position;
    if (Cursor->Position == Start || (Cursor->Position == Start + 1U && (Cursor->Data[Start] == '-' || Cursor->Data[Start] == '+'))) {
        GroovJsonError(Cursor, "expected integer");
        return false;
    }
    Length = Cursor->Position - Start;
    Number = malloc(Length + 1U);
    if (Number == NULL) {
        GroovVkSetError(Cursor->Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot allocate JSON number");
        return false;
    }
    memcpy(Number, Cursor->Data + Start, Length);
    Number[Length] = '\0';
    errno = 0;
    Parsed = strtoll(Number, &End, 10);
    if (errno == ERANGE || End == NULL || *End != '\0') {
        free(Number);
        GroovJsonError(Cursor, "invalid integer");
        return false;
    }
    free(Number);
    *Value = Parsed;
    return true;
}

static bool GroovJsonReadTrack(GroovJsonCursor *Cursor, GroovVkTrack *Track)
{
    bool First = true;
    char *Key = NULL;
    char *StringValue = NULL;
    long long Number;

    memset(Track, 0, sizeof(*Track));
    if (!GroovJsonConsume(Cursor, '{')) return false;
    GroovJsonSkipWhitespace(Cursor);
    if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == '}') {
        ++Cursor->Position;
        return true;
    }
    while (Cursor->Position < Cursor->Length) {
        if (!First && !GroovJsonConsume(Cursor, ',')) goto Failed;
        First = false;
        if (!GroovJsonReadString(Cursor, &Key) || !GroovJsonConsume(Cursor, ':')) goto Failed;
        if (strcmp(Key, "id") == 0 || strcmp(Key, "owner_id") == 0 || strcmp(Key, "duration") == 0) {
            if (!GroovJsonReadInteger(Cursor, &Number)) goto Failed;
            if (strcmp(Key, "id") == 0) Track->Id = Number;
            else if (strcmp(Key, "owner_id") == 0) Track->OwnerId = Number;
            else if (Number >= 0) Track->DurationSeconds = (unsigned int)((Number > UINT_MAX) ? UINT_MAX : Number);
        } else if (strcmp(Key, "artist") == 0 || strcmp(Key, "title") == 0 || strcmp(Key, "url") == 0) {
            if (!GroovJsonReadString(Cursor, &StringValue)) goto Failed;
            if (strcmp(Key, "artist") == 0) { free(Track->Artist); Track->Artist = StringValue; }
            else if (strcmp(Key, "title") == 0) { free(Track->Title); Track->Title = StringValue; }
            else { free(Track->Url); Track->Url = StringValue; }
            StringValue = NULL;
        } else if (!GroovJsonSkipValue(Cursor)) goto Failed;
        free(Key); Key = NULL;
        GroovJsonSkipWhitespace(Cursor);
        if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == '}') {
            ++Cursor->Position;
            return true;
        }
    }
    GroovJsonError(Cursor, "unterminated audio object");
Failed:
    free(Key);
    free(StringValue);
    GroovVkTrackDestroy(Track);
    return false;
}

static bool GroovJsonReadItems(GroovJsonCursor *Cursor, GroovVkTrackList *Tracks)
{
    bool First = true;
    GroovVkTrack Track;

    if (!GroovJsonConsume(Cursor, '[')) return false;
    GroovJsonSkipWhitespace(Cursor);
    if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == ']') {
        ++Cursor->Position;
        return true;
    }
    while (Cursor->Position < Cursor->Length) {
        if (!First && !GroovJsonConsume(Cursor, ',')) return false;
        First = false;
        if (!GroovJsonReadTrack(Cursor, &Track)) return false;
        if (GroovVkTrackListAppend(Tracks, &Track, Cursor->Error) != GROOV_VK_STATUS_OK) {
            GroovVkTrackDestroy(&Track);
            return false;
        }
        GroovJsonSkipWhitespace(Cursor);
        if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == ']') {
            ++Cursor->Position;
            return true;
        }
    }
    GroovJsonError(Cursor, "unterminated audio list");
    return false;
}

static bool GroovJsonReadResponse(GroovJsonCursor *Cursor, GroovVkTrackList *Tracks)
{
    bool First = true;
    char *Key = NULL;
    long long Number;

    if (!GroovJsonConsume(Cursor, '{')) return false;
    GroovJsonSkipWhitespace(Cursor);
    if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == '}') {
        ++Cursor->Position;
        return true;
    }
    while (Cursor->Position < Cursor->Length) {
        if (!First && !GroovJsonConsume(Cursor, ',')) goto Failed;
        First = false;
        if (!GroovJsonReadString(Cursor, &Key) || !GroovJsonConsume(Cursor, ':')) goto Failed;
        if (strcmp(Key, "count") == 0) {
            if (!GroovJsonReadInteger(Cursor, &Number)) goto Failed;
            Tracks->TotalCount = (Number <= 0) ? 0U : (Number > UINT_MAX ? UINT_MAX : (unsigned int)Number);
        } else if (strcmp(Key, "items") == 0) {
            if (!GroovJsonReadItems(Cursor, Tracks)) goto Failed;
        } else if (!GroovJsonSkipValue(Cursor)) goto Failed;
        free(Key); Key = NULL;
        GroovJsonSkipWhitespace(Cursor);
        if (Cursor->Position < Cursor->Length && Cursor->Data[Cursor->Position] == '}') {
            ++Cursor->Position;
            return true;
        }
    }
    GroovJsonError(Cursor, "unterminated VK response");
Failed:
    free(Key);
    return false;
}

GroovVkStatus GroovVkParseAudioResponse(
    const char *Json,
    size_t JsonLength,
    GroovVkTrackList *Tracks,
    GroovVkError *Error
)
{
    GroovJsonCursor Cursor;
    bool First = true;
    bool HasResponse = false;
    char *Key = NULL;
    char *ErrorMessage = NULL;
    long long Number;

    if (Json == NULL || Tracks == NULL) {
        GroovVkSetError(Error, GROOV_VK_STATUS_INVALID_ARGUMENT, "JSON and track list are required");
        return GROOV_VK_STATUS_INVALID_ARGUMENT;
    }
    GroovVkErrorClear(Error);
    GroovVkTrackListDestroy(Tracks);
    (void)GroovVkTrackListInit(Tracks, Error);
    Cursor.Data = Json;
    Cursor.Length = JsonLength;
    Cursor.Position = 0U;
    Cursor.Error = Error;
    if (!GroovJsonConsume(&Cursor, '{')) return Error->Status;
    GroovJsonSkipWhitespace(&Cursor);
    if (Cursor.Position < Cursor.Length && Cursor.Data[Cursor.Position] == '}') {
        GroovJsonError(&Cursor, "VK response has no response object");
        return Error->Status;
    }
    while (Cursor.Position < Cursor.Length) {
        if (!First && !GroovJsonConsume(&Cursor, ',')) goto Failed;
        First = false;
        if (!GroovJsonReadString(&Cursor, &Key) || !GroovJsonConsume(&Cursor, ':')) goto Failed;
        if (strcmp(Key, "response") == 0) {
            if (!GroovJsonReadResponse(&Cursor, Tracks)) goto Failed;
            HasResponse = true;
        } else if (strcmp(Key, "error") == 0) {
            bool ErrorFirst = true;
            if (!GroovJsonConsume(&Cursor, '{')) goto Failed;
            while (Cursor.Position < Cursor.Length) {
                char *ErrorKey = NULL;
                if (!ErrorFirst && !GroovJsonConsume(&Cursor, ',')) { free(ErrorKey); goto Failed; }
                ErrorFirst = false;
                if (!GroovJsonReadString(&Cursor, &ErrorKey) || !GroovJsonConsume(&Cursor, ':')) { free(ErrorKey); goto Failed; }
                if (strcmp(ErrorKey, "error_code") == 0) {
                    if (!GroovJsonReadInteger(&Cursor, &Number)) { free(ErrorKey); goto Failed; }
                    Error->ApiCode = (int)Number;
                } else if (strcmp(ErrorKey, "error_msg") == 0) {
                    if (!GroovJsonReadString(&Cursor, &ErrorMessage)) { free(ErrorKey); goto Failed; }
                } else if (!GroovJsonSkipValue(&Cursor)) { free(ErrorKey); goto Failed; }
                free(ErrorKey);
                GroovJsonSkipWhitespace(&Cursor);
                if (Cursor.Position < Cursor.Length && Cursor.Data[Cursor.Position] == '}') { ++Cursor.Position; break; }
            }
        } else if (!GroovJsonSkipValue(&Cursor)) goto Failed;
        free(Key); Key = NULL;
        GroovJsonSkipWhitespace(&Cursor);
        if (Cursor.Position < Cursor.Length && Cursor.Data[Cursor.Position] == '}') { ++Cursor.Position; break; }
    }
    GroovJsonSkipWhitespace(&Cursor);
    if (Cursor.Position != Cursor.Length) {
        GroovJsonError(&Cursor, "trailing data after VK response");
        goto Failed;
    }
    if (Error->ApiCode != 0) {
        GroovVkSetError(Error, GROOV_VK_STATUS_API, "%s", ErrorMessage == NULL ? "VK API request failed" : ErrorMessage);
        free(ErrorMessage);
        return Error->Status;
    }
    free(ErrorMessage);
    if (!HasResponse) {
        GroovVkSetError(Error, GROOV_VK_STATUS_PARSE, "VK response object is missing");
        GroovVkTrackListDestroy(Tracks);
        return Error->Status;
    }
    return GROOV_VK_STATUS_OK;

Failed:
    free(Key);
    free(ErrorMessage);
    GroovVkTrackListDestroy(Tracks);
    return Error->Status == GROOV_VK_STATUS_OK ? GROOV_VK_STATUS_PARSE : Error->Status;
}

typedef struct GroovHttpBuffer {
    char *Data;
    size_t Length;
    size_t Capacity;
} GroovHttpBuffer;

static size_t GroovHttpWrite(void *Contents, size_t Size, size_t Count, void *UserData)
{
    GroovHttpBuffer *Buffer = UserData;
    size_t Incoming = Size * Count;
    size_t NewCapacity;
    char *NewData;

    if (Incoming > SIZE_MAX - Buffer->Length - 1U) return 0U;
    if (Buffer->Length + Incoming + 1U > Buffer->Capacity) {
        NewCapacity = (Buffer->Capacity == 0U) ? 4096U : Buffer->Capacity;
        while (NewCapacity < Buffer->Length + Incoming + 1U) {
            if (NewCapacity > SIZE_MAX / 2U) return 0U;
            NewCapacity *= 2U;
        }
        NewData = realloc(Buffer->Data, NewCapacity);
        if (NewData == NULL) return 0U;
        Buffer->Data = NewData;
        Buffer->Capacity = NewCapacity;
    }
    memcpy(Buffer->Data + Buffer->Length, Contents, Incoming);
    Buffer->Length += Incoming;
    Buffer->Data[Buffer->Length] = '\0';
    return Incoming;
}

GroovVkStatus GroovVkClientListAudio(
    const GroovVkClient *Client,
    unsigned int Offset,
    unsigned int Count,
    GroovVkTrackList *Tracks,
    GroovVkError *Error
)
{
    CURL *Curl = NULL;
    CURLcode CurlResult;
    char *EscapedToken = NULL;
    char *EscapedVersion = NULL;
    char *Url = NULL;
    size_t UrlLength;
    long HttpStatus = 0L;
    GroovHttpBuffer Body = {0};
    GroovVkStatus Status;

    if (Client == NULL || Client->AccessToken == NULL || Tracks == NULL) {
        GroovVkSetError(Error, GROOV_VK_STATUS_INVALID_ARGUMENT, "client and track list are required");
        return GROOV_VK_STATUS_INVALID_ARGUMENT;
    }
    if (Count == 0U) Count = GROOV_VK_DEFAULT_PAGE_SIZE;
    if (Count > GROOV_VK_MAX_PAGE_SIZE) Count = GROOV_VK_MAX_PAGE_SIZE;
    GroovVkErrorClear(Error);
    GroovVkTrackListDestroy(Tracks);
    (void)GroovVkTrackListInit(Tracks, Error);
    (void)curl_global_init(CURL_GLOBAL_DEFAULT);
    Curl = curl_easy_init();
    if (Curl == NULL) {
        GroovVkSetError(Error, GROOV_VK_STATUS_NETWORK, "cannot initialize libcurl");
        return Error->Status;
    }
    EscapedToken = curl_easy_escape(Curl, Client->AccessToken, 0);
    EscapedVersion = curl_easy_escape(Curl, Client->ApiVersion, 0);
    if (EscapedToken == NULL || EscapedVersion == NULL) {
        GroovVkSetError(Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot encode VK request");
        goto Cleanup;
    }
    UrlLength = strlen(Client->BaseUrl) + strlen(EscapedToken) + strlen(EscapedVersion) + 128U;
    Url = malloc(UrlLength);
    if (Url == NULL) {
        GroovVkSetError(Error, GROOV_VK_STATUS_OUT_OF_MEMORY, "cannot allocate VK request URL");
        goto Cleanup;
    }
    (void)snprintf(Url, UrlLength, "%s?access_token=%s&v=%s&offset=%u&count=%u",
                   Client->BaseUrl, EscapedToken, EscapedVersion, Offset, Count);
    (void)curl_easy_setopt(Curl, CURLOPT_URL, Url);
    (void)curl_easy_setopt(Curl, CURLOPT_USERAGENT, "groov/0.1 (+https://github.com/groov-player)");
    (void)curl_easy_setopt(Curl, CURLOPT_WRITEFUNCTION, GroovHttpWrite);
    (void)curl_easy_setopt(Curl, CURLOPT_WRITEDATA, &Body);
    (void)curl_easy_setopt(Curl, CURLOPT_CONNECTTIMEOUT, Client->ConnectTimeoutSeconds);
    (void)curl_easy_setopt(Curl, CURLOPT_TIMEOUT, Client->RequestTimeoutSeconds);
    (void)curl_easy_setopt(Curl, CURLOPT_FOLLOWLOCATION, 0L);
    CurlResult = curl_easy_perform(Curl);
    if (CurlResult != CURLE_OK) {
        GroovVkSetError(Error, GROOV_VK_STATUS_NETWORK, "VK request failed: %s", curl_easy_strerror(CurlResult));
        goto Cleanup;
    }
    (void)curl_easy_getinfo(Curl, CURLINFO_RESPONSE_CODE, &HttpStatus);
    Error->HttpStatus = HttpStatus;
    if (HttpStatus < 200L || HttpStatus >= 300L) {
        GroovVkSetError(Error, GROOV_VK_STATUS_HTTP, "VK returned HTTP status %ld", HttpStatus);
        goto Cleanup;
    }
    Status = GroovVkParseAudioResponse(Body.Data, Body.Length, Tracks, Error);
    if (Status != GROOV_VK_STATUS_OK) goto Cleanup;

Cleanup:
    curl_free(EscapedToken);
    curl_free(EscapedVersion);
    free(Url);
    free(Body.Data);
    curl_easy_cleanup(Curl);
    return Error->Status;
}
