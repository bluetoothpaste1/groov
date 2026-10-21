#include "groov/vk.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void GroovPrintUsage(const char *Program)
{
    (void)printf("Usage: %s tracks [options]\n\n", Program);
    (void)printf("Fetch the current user's VK audio library.\n\n");
    (void)printf("Options:\n");
    (void)printf("  --token TOKEN       VK API access token (prefer GROOV_VK_ACCESS_TOKEN)\n");
    (void)printf("  --offset NUMBER     Number of tracks to skip (default: 0)\n");
    (void)printf("  --count NUMBER      Tracks per request (default: %u, max: %u)\n", GROOV_VK_DEFAULT_PAGE_SIZE, GROOV_VK_MAX_PAGE_SIZE);
    (void)printf("  --help              Show this help\n");
}

static int GroovParseUnsigned(const char *Text, unsigned int *Value)
{
    char *End = NULL;
    unsigned long Parsed;

    if (Text == NULL || Text[0] == '\0') return 0;
    Parsed = strtoul(Text, &End, 10);
    if (*End != '\0' || Parsed > 4294967295UL) return 0;
    *Value = (unsigned int)Parsed;
    return 1;
}

static void GroovPrintError(const GroovVkError *Error)
{
    (void)fprintf(stderr, "groov: %s", Error->Message[0] == '\0' ? GroovVkStatusName(Error->Status) : Error->Message);
    if (Error->ApiCode != 0) (void)fprintf(stderr, " (VK error %d)", Error->ApiCode);
    if (Error->HttpStatus != 0L) (void)fprintf(stderr, " (HTTP %ld)", Error->HttpStatus);
    (void)fputc('\n', stderr);
}

static void GroovPrintDuration(unsigned int Seconds)
{
    (void)printf("%02u:%02u", Seconds / 60U, Seconds % 60U);
}

static int GroovRunTracks(int ArgumentCount, char **Arguments)
{
    const char *Token = getenv("GROOV_VK_ACCESS_TOKEN");
    unsigned int Offset = 0U;
    unsigned int Count = GROOV_VK_DEFAULT_PAGE_SIZE;
    unsigned int Index;
    GroovVkClient Client;
    GroovVkTrackList Tracks = {0};
    GroovVkError Error;
    GroovVkStatus Status;

    for (Index = 0U; Index < (unsigned int)ArgumentCount; ++Index) {
        if (strcmp(Arguments[Index], "--help") == 0) {
            GroovPrintUsage("groov");
            return 0;
        }
        if (strcmp(Arguments[Index], "--token") == 0) {
            if (Index + 1U >= (unsigned int)ArgumentCount) {
                (void)fprintf(stderr, "groov: --token requires a value\n");
                return 2;
            }
            Token = Arguments[++Index];
        } else if (strcmp(Arguments[Index], "--offset") == 0) {
            if (Index + 1U >= (unsigned int)ArgumentCount || !GroovParseUnsigned(Arguments[++Index], &Offset)) {
                (void)fprintf(stderr, "groov: --offset must be an unsigned integer\n");
                return 2;
            }
        } else if (strcmp(Arguments[Index], "--count") == 0) {
            if (Index + 1U >= (unsigned int)ArgumentCount || !GroovParseUnsigned(Arguments[++Index], &Count)) {
                (void)fprintf(stderr, "groov: --count must be an unsigned integer\n");
                return 2;
            }
        } else {
            (void)fprintf(stderr, "groov: unknown option '%s'\n", Arguments[Index]);
            return 2;
        }
    }
    if (Token == NULL || Token[0] == '\0') {
        (void)fprintf(stderr, "groov: VK access token is missing; set GROOV_VK_ACCESS_TOKEN or use --token\n");
        return 2;
    }
    Status = GroovVkClientInit(&Client, Token, NULL, &Error);
    if (Status != GROOV_VK_STATUS_OK) {
        GroovPrintError(&Error);
        return 1;
    }
    Status = GroovVkClientListAudio(&Client, Offset, Count, &Tracks, &Error);
    if (Status != GROOV_VK_STATUS_OK) {
        GroovPrintError(&Error);
        GroovVkClientDestroy(&Client);
        GroovVkTrackListDestroy(&Tracks);
        return 1;
    }
    (void)printf("Tracks: %zu of %u\n", Tracks.Length, Tracks.TotalCount);
    for (Index = 0U; Index < (unsigned int)Tracks.Length; ++Index) {
        const GroovVkTrack *Track = &Tracks.Items[Index];
        (void)printf("%4u  ", Index + 1U);
        GroovPrintDuration(Track->DurationSeconds);
        (void)printf("  %s — %s\n", Track->Artist == NULL ? "Unknown artist" : Track->Artist,
                     Track->Title == NULL ? "Untitled" : Track->Title);
    }
    GroovVkClientDestroy(&Client);
    GroovVkTrackListDestroy(&Tracks);
    return 0;
}

int main(int ArgumentCount, char **Arguments)
{
    if (ArgumentCount < 2 || strcmp(Arguments[1], "--help") == 0) {
        GroovPrintUsage(Arguments[0]);
        return ArgumentCount < 2 ? 0 : 0;
    }
    if (strcmp(Arguments[1], "tracks") == 0) {
        return GroovRunTracks(ArgumentCount - 2, Arguments + 2);
    }
    (void)fprintf(stderr, "groov: unknown command '%s'\n\n", Arguments[1]);
    GroovPrintUsage(Arguments[0]);
    return 2;
}
