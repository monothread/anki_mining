/* Speaks a text with VOICEVOX CORE and writes the WAV to a file.
 *
 *   voicevox_say <resources dir> <style id> <text> <output.wav>
 *   voicevox_say <resources dir> list
 *
 * <resources dir> holds libvoicevox_core.so, the VOICEVOX ONNX Runtime library, the Open JTalk
 * dictionary (open_jtalk_dic_utf_8-1.11/) and the voice models (the vvms folder). It runs as its own
 * process so its ONNX Runtime never meets the one the app uses for OCR. */
#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "voicevox_core.h"

static int fail(const char *step, VoicevoxResultCode code) {
    fprintf(stderr, "%s: %s\n", step, voicevox_error_result_to_message(code));
    return 1;
}

static int path_join(char *out, size_t size, const char *a, const char *b) {
    return snprintf(out, size, "%s/%s", a, b) >= (int)size;
}

/* The ONNX Runtime file that sits in the resources folder, whatever its version suffix is. */
static int find_runtime(char *out, size_t size, const char *dir) {
    DIR *handle = opendir(dir);
    if (!handle) return 1;
    struct dirent *entry;
    int found = 1;
    while (found && (entry = readdir(handle)) != NULL) {
        if (strncmp(entry->d_name, "libvoicevox_onnxruntime.so", 26) == 0) {
            found = path_join(out, size, dir, entry->d_name);
        }
    }
    closedir(handle);
    return found;
}

static int load_models(const VoicevoxSynthesizer *synthesizer, const char *dir) {
    char vvms[4096];
    if (path_join(vvms, sizeof vvms, dir, "vvms")) return 1;
    DIR *handle = opendir(vvms);
    if (!handle) {
        fprintf(stderr, "cannot open %s\n", vvms);
        return 1;
    }
    struct dirent *entry;
    int loaded = 0;
    while ((entry = readdir(handle)) != NULL) {
        size_t length = strlen(entry->d_name);
        if (length < 5 || strcmp(entry->d_name + length - 4, ".vvm") != 0) continue;
        char file[4096];
        if (path_join(file, sizeof file, vvms, entry->d_name)) continue;
        VoicevoxVoiceModelFile *model = NULL;
        VoicevoxResultCode code = voicevox_voice_model_file_open(file, &model);
        if (code != VOICEVOX_RESULT_OK) return fail("open model", code);
        code = voicevox_synthesizer_load_voice_model(synthesizer, model, voicevox_make_default_load_voice_model_options());
        voicevox_voice_model_file_delete(model);
        if (code != VOICEVOX_RESULT_OK) return fail("load model", code);
        loaded++;
    }
    closedir(handle);
    if (loaded == 0) {
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
        fprintf(stderr, "no libvoicevox_onnxruntime.so* in %s\n", dir);
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
