#import <Foundation/Foundation.h>
#import <Speech/Speech.h>
#import <AVFoundation/AVFoundation.h>

typedef struct {
    char *json_result;
    bool success;
} STTResult;

#ifdef __cplusplus
extern "C" {
#endif

bool ios_speech_request_authorization(void) {
    __block bool granted = false;
    dispatch_semaphore_t sem = dispatch_semaphore_create(0);
    [SFSpeechRecognizer requestAuthorization:^(SFSpeechRecognizerAuthorizationStatus status) {
        if (status == SFSpeechRecognizerAuthorizationStatusAuthorized) {
            granted = true;
        }
        dispatch_semaphore_signal(sem);
    }];
    dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));
    return granted;
}

STTResult ios_speech_transcribe_file(const char *file_path_c, const char *lang_c) {
    STTResult res;
    res.json_result = NULL;
    res.success = false;

    if (!file_path_c) {
        res.json_result = strdup("File path is null");
        return res;
    }

    NSString *filePath = [NSString stringWithUTF8String:file_path_c];
    NSFileManager *fileManager = [NSFileManager defaultManager];
    if (![fileManager fileExistsAtPath:filePath]) {
        res.json_result = strdup("File does not exist at path");
        return res;
    }

    NSString *langStr = (lang_c && strlen(lang_c) > 0) ? [NSString stringWithUTF8String:lang_c] : @"en-US";
    NSLocale *locale = [NSLocale localeWithLocaleIdentifier:langStr];
    SFSpeechRecognizer *recognizer = [[SFSpeechRecognizer alloc] initWithLocale:locale];
    if (!recognizer) {
        recognizer = [[SFSpeechRecognizer alloc] initWithLocale:[NSLocale localeWithLocaleIdentifier:@"en-US"]];
    }

    if (!recognizer || !recognizer.isAvailable) {
        res.json_result = strdup("SFSpeechRecognizer is not available on device");
        return res;
    }

    NSURL *url = [NSURL fileURLWithPath:filePath];
    SFSpeechURLRecognitionRequest *request = [[SFSpeechURLRecognitionRequest alloc] initWithURL:url];
    
    // GUARANTEE 100% offline transcription on iOS
    request.requiresOnDeviceRecognition = YES;
    request.shouldReportPartialResults = NO;

    dispatch_semaphore_t sem = dispatch_semaphore_create(0);
    __block NSString *jsonOutput = nil;
    __block NSError *recognitionError = nil;

    [recognizer recognitionTaskWithRequest:request resultHandler:^(SFSpeechRecognitionResult * _Nullable result, NSError * _Nullable error) {
        if (error) {
            recognitionError = error;
            dispatch_semaphore_signal(sem);
            return;
        }

        if (result && result.isFinal) {
            NSMutableArray *segmentsArray = [NSMutableArray array];
            SFTranscription *transcription = result.bestTranscription;

            for (SFTranscriptionSegment *seg in transcription.segments) {
                NSDictionary *segDict = @{
                    @"start_timestamp": @(seg.timestamp),
                    @"end_timestamp": @(seg.timestamp + seg.duration),
                    @"text": seg.substring ?: @"",
                    @"speaker": @"Speaker 1"
                };
                [segmentsArray addObject:segDict];
            }

            if (segmentsArray.count == 0 && transcription.formattedString.length > 0) {
                NSDictionary *segDict = @{
                    @"start_timestamp": @(0.0),
                    @"end_timestamp": @(0.0),
                    @"text": transcription.formattedString,
                    @"speaker": @"Speaker 1"
                };
                [segmentsArray addObject:segDict];
            }

            NSData *jsonData = [NSJSONSerialization dataWithJSONObject:segmentsArray options:0 error:nil];
            if (jsonData) {
                jsonOutput = [[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding];
            } else {
                jsonOutput = @"[]";
            }
            dispatch_semaphore_signal(sem);
        }
    }];

    long waitRes = dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 120 * NSEC_PER_SEC));

    if (waitRes != 0) {
        res.json_result = strdup("Speech recognition timed out");
        res.success = false;
        return res;
    }

    if (recognitionError) {
        const char *errStr = [[recognitionError localizedDescription] UTF8String];
        res.json_result = strdup(errStr ? errStr : "Unknown speech recognition error");
        res.success = false;
        return res;
    }

    if (jsonOutput) {
        res.json_result = strdup([jsonOutput UTF8String]);
        res.success = true;
    } else {
        res.json_result = strdup("[]");
        res.success = true;
    }

    return res;
}

STTResult ios_speech_transcribe_buffer(const float *pcm_data, size_t sample_count, uint32_t sample_rate, const char *lang_c) {
    STTResult res;
    res.json_result = NULL;
    res.success = false;

    if (!pcm_data || sample_count == 0) {
        res.json_result = strdup("[]");
        res.success = true;
        return res;
    }

    NSString *tempDir = NSTemporaryDirectory();
    NSString *tempPath = [tempDir stringByAppendingPathComponent:[NSString stringWithFormat:@"stt_buf_%@.wav", [[NSUUID UUID] UUIDString]]];

    uint32_t sr = (sample_rate > 0) ? sample_rate : 16000;
    uint16_t numChannels = 1;
    uint16_t bitsPerSample = 16;
    uint32_t dataSize = (uint32_t)(sample_count * sizeof(int16_t));
    uint32_t chunkSize = 36 + dataSize;
    uint32_t byteRate = sr * numChannels * (bitsPerSample / 8);
    uint16_t blockAlign = numChannels * (bitsPerSample / 8);

    NSMutableData *wavData = [NSMutableData dataWithCapacity:44 + dataSize];
    [wavData appendBytes:"RIFF" length:4];
    [wavData appendBytes:&chunkSize length:4];
    [wavData appendBytes:"WAVEfmt " length:8];
    uint32_t fmtChunkSize = 16;
    uint16_t audioFormat = 1;
    [wavData appendBytes:&fmtChunkSize length:4];
    [wavData appendBytes:&audioFormat length:2];
    [wavData appendBytes:&numChannels length:2];
    [wavData appendBytes:&sr length:4];
    [wavData appendBytes:&byteRate length:4];
    [wavData appendBytes:&blockAlign length:2];
    [wavData appendBytes:&bitsPerSample length:2];
    [wavData appendBytes:"data" length:4];
    [wavData appendBytes:&dataSize length:4];

    int16_t *i16Buffer = (int16_t *)malloc(dataSize);
    if (!i16Buffer) {
        res.json_result = strdup("Memory allocation failure for audio conversion");
        return res;
    }

    for (size_t i = 0; i < sample_count; i++) {
        float sample = pcm_data[i];
        if (sample < -1.0f) sample = -1.0f;
        if (sample > 1.0f) sample = 1.0f;
        i16Buffer[i] = (int16_t)(sample * 32767.0f);
    }
    [wavData appendBytes:i16Buffer length:dataSize];
    free(i16Buffer);

    NSError *writeErr = nil;
    [wavData writeToFile:tempPath options:NSDataWritingAtomic error:&writeErr];
    if (writeErr) {
        res.json_result = strdup([[writeErr localizedDescription] UTF8String]);
        return res;
    }

    STTResult fileRes = ios_speech_transcribe_file([tempPath UTF8String], lang_c);

    [[NSFileManager defaultManager] removeItemAtPath:tempPath error:nil];
    return fileRes;
}

void ios_speech_free_result(STTResult res) {
    if (res.json_result) {
        free(res.json_result);
    }
}

#ifdef __cplusplus
}
#endif
