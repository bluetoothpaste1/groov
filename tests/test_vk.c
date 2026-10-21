#include "groov/vk.h"

#include <assert.h>
#include <string.h>

static void TestAudioResponse(void)
{
    const char Json[] =
        "{\"response\":{\"count\":2,\"items\":["
        "{\"id\":11,\"owner_id\":22,\"artist\":\"Björk\","
        "\"title\":\"Jóga \\uD83C\\uDFB5\",\"duration\":245,\"url\":\"https://example.test/a.mp3\"},"
        "{\"id\":12,\"owner_id\":22,\"artist\":\"Second\",\"title\":\"Track\",\"duration\":0}]}}";
    GroovVkTrackList Tracks = {0};
    GroovVkError Error;

    assert(GroovVkParseAudioResponse(Json, strlen(Json), &Tracks, &Error) == GROOV_VK_STATUS_OK);
    assert(Tracks.TotalCount == 2U);
    assert(Tracks.Length == 2U);
    assert(strcmp(Tracks.Items[0].Artist, "Björk") == 0);
    assert(strcmp(Tracks.Items[0].Title, "Jóga 🎵") == 0);
    assert(Tracks.Items[0].DurationSeconds == 245U);
    assert(Tracks.Items[1].Url == NULL);
    GroovVkTrackListDestroy(&Tracks);
}

static void TestApiError(void)
{
    const char Json[] = "{\"error\":{\"error_code\":5,\"error_msg\":\"User authorization failed\"}}";
    GroovVkTrackList Tracks = {0};
    GroovVkError Error;

    assert(GroovVkParseAudioResponse(Json, strlen(Json), &Tracks, &Error) == GROOV_VK_STATUS_API);
    assert(Error.ApiCode == 5);
    assert(strcmp(Error.Message, "User authorization failed") == 0);
    GroovVkTrackListDestroy(&Tracks);
}

static void TestMalformedResponse(void)
{
    const char Json[] = "{\"response\":{\"items\":[}";
    GroovVkTrackList Tracks = {0};
    GroovVkError Error;

    assert(GroovVkParseAudioResponse(Json, strlen(Json), &Tracks, &Error) == GROOV_VK_STATUS_PARSE);
    GroovVkTrackListDestroy(&Tracks);
}

int main(void)
{
    TestAudioResponse();
    TestApiError();
    TestMalformedResponse();
    return 0;
}
