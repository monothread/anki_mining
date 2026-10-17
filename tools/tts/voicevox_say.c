/* Speaks a text with VOICEVOX CORE and writes the WAV to a file.
 *
 *   voicevox_say <resources dir> <style id> <text> <output.wav>
 *   voicevox_say <resources dir> list
 *
 * <resources dir> holds the VOICEVOX CORE library, the VOICEVOX ONNX Runtime library, the Open
 * JTalk dictionary (open_jtalk_dic_utf_8-1.11/) and the voice models (the vvms folder). It runs as
 * its own process so its ONNX Runtime never meets the one the app uses for OCR. It builds on
 * Linux (gcc) and on Windows (mingw-w64 or MSVC). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#define SEPARATOR "\\"
#define RUNTIME_PREFIX "voicevox_onnxruntime.dll"
#else
#include <dirent.h>
#define SEPARATOR "/"
#define RUNTIME_PREFIX "libvoicevox_onnxruntime.so"
#endif

#include "voicevox_core.h"

typedef int (*FileVisitor)(const char *name, void *context);

static int fail(const char *step, VoicevoxResultCode code) {
    fprintf(stderr, "%s: %s\n", step, voicevox_error_result_to_message(code));
    return 1;
}

static int path_join(char *out, size_t size, const char *a, const char *b) {
    return snprintf(out, size, "%s" SEPARATOR "%s", a, b) >= (int)size;
}

/* Calls `visitor` with the name of every file in `dir`; stops when it returns non-zero (and
 * returns that value). Returns -1 when the folder cannot be read. */
static int for_each_file(const char *dir, FileVisitor visitor, void *context) {
#ifdef _WIN32
    char pattern[4096];
    WIN32_FIND_DATAA found;
    if (path_join(pattern, sizeof pattern, dir, "*")) return -1;
    HANDLE handle = FindFirstFileA(pattern, &found);
    if (handle == INVALID_HANDLE_VALUE) return -1;
    int result = 0;
    do {
        if (!(found.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)) {
            result = visitor(found.cFileName, context);
        }
    } while (result == 0 && FindNextFileA(handle, &found));
    FindClose(handle);
    return result;
#else
    DIR *handle = opendir(dir);
    if (!handle) return -1;
    struct dirent *entry;
    int result = 0;
    while (result == 0 && (entry = readdir(handle)) != NULL) {
        result = visitor(entry->d_name, context);
    }
    closedir(handle);
    return result;
#endif
}

struct RuntimeSearch {
    const char *dir;
    char *out;
    size_t size;
};

/* The ONNX Runtime file that sits in the resources folder, whatever its version suffix is. */
static int match_runtime(const char *name, void *context) {
    struct RuntimeSearch *search = context;
    if (strncmp(name, RUNTIME_PREFIX, strlen(RUNTIME_PREFIX)) != 0) return 0;
    return path_join(search->out, search->size, search->dir, name) ? 2 : 1;
}

static int find_runtime(char *out, size_t size, const char *dir) {
    struct RuntimeSearch search = {dir, out, size};
    return for_each_file(dir, match_runtime, &search) == 1 ? 0 : 1;
}

struct ModelLoad {
    const VoicevoxSynthesizer *synthesizer;
    const char *dir;
    int loaded;
};

static int load_model(const char *name, void *context) {
    struct ModelLoad *load = context;
    size_t length = strlen(name);
    if (length < 5 || strcmp(name + length - 4, ".vvm") != 0) return 0;
    char file[4096];
    if (path_join(file, sizeof file, load->dir, name)) return 0;
    VoicevoxVoiceModelFile *model = NULL;
    VoicevoxResultCode code = voicevox_voice_model_file_open(file, &model);
    if (code != VOICEVOX_RESULT_OK) return fail("open model", code);
    code = voicevox_synthesizer_load_voice_model(load->synthesizer, model,
                                                 voicevox_make_default_load_voice_model_options());
    voicevox_voice_model_file_delete(model);
    if (code != VOICEVOX_RESULT_OK) return fail("load model", code);
    load->loaded++;
    return 0;
}

static int load_models(const VoicevoxSynthesizer *synthesizer, const char *dir) {
    char vvms[4096];
    if (path_join(vvms, sizeof vvms, dir, "vvms")) return 1;
    struct ModelLoad load = {synthesizer, vvms, 0};
    int result = for_each_file(vvms, load_model, &load);
    if (result < 0) {
        fprintf(stderr, "cannot open %s\n", vvms);
        return 1;
    }
    if (result != 0) return 1;
    if (load.loaded == 0) {
        fprintf(stderr, "no .vvm voice model in %s\n", vvms);
        return 1;
    }
    return 0;
}

int main(int argc, char **argv) {
    int listing = argc == 3 && strcmp(argv[2], "list") == 0;
    if (!listing && argc != 5) {
        fprintf(stderr, "usage: voicevox_say <dir> <style id> <text> <output.wav> | <dir> list\n");
        return 2;
    }
    const char *dir = argv[1];
    char runtime[4096], dictionary[4096];
    if (find_runtime(runtime, sizeof runtime, dir)) {
        fprintf(stderr, "no %s* in %s\n", RUNTIME_PREFIX, dir);
        return 2;
    }
    if (path_join(dictionary, sizeof dictionary, dir, "open_jtalk_dic_utf_8-1.11")) return 2;

    struct VoicevoxLoadOnnxruntimeOptions ort_options = voicevox_make_default_load_onnxruntime_options();
    ort_options.filename = runtime;
    const VoicevoxOnnxruntime *onnxruntime = NULL;
    VoicevoxResultCode code = voicevox_onnxruntime_load_once(ort_options, &onnxruntime);
    if (code != VOICEVOX_RESULT_OK) return fail("onnxruntime", code);

    OpenJtalkRc *open_jtalk = NULL;
    code = voicevox_open_jtalk_rc_new(dictionary, &open_jtalk);
    if (code != VOICEVOX_RESULT_OK) return fail("open_jtalk", code);

    VoicevoxSynthesizer *synthesizer = NULL;
    struct VoicevoxInitializeOptions options = voicevox_make_default_initialize_options();
    code = voicevox_synthesizer_new(onnxruntime, open_jtalk, options, &synthesizer);
    if (code != VOICEVOX_RESULT_OK) return fail("synthesizer", code);

    if (load_models(synthesizer, dir)) return 1;

    if (listing) {
        char *metas = voicevox_synthesizer_create_metas_json(synthesizer);
        puts(metas);
        voicevox_json_free(metas);
        return 0;
    }

    uintptr_t length = 0;
    uint8_t *wav = NULL;
    struct VoicevoxTtsOptions tts = voicevox_make_default_tts_options();
    code = voicevox_synthesizer_tts(synthesizer, argv[3], (VoicevoxStyleId)atoi(argv[2]), tts, &length, &wav);
    if (code != VOICEVOX_RESULT_OK) return fail("tts", code);

    FILE *out = fopen(argv[4], "wb");
    if (!out || fwrite(wav, 1, length, out) != length) {
        fprintf(stderr, "cannot write %s\n", argv[4]);
        return 1;
    }
    fclose(out);
    voicevox_wav_free(wav);
    return 0;
}
