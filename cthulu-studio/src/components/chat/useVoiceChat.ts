import { useState, useRef, useCallback, useEffect } from "react";

// Web Speech API types (not included in all TS libs)
interface SpeechRecognitionResult {
  readonly isFinal: boolean;
  readonly length: number;
  [index: number]: { readonly transcript: string; readonly confidence: number };
}
interface SpeechRecognitionResultList {
  readonly length: number;
  [index: number]: SpeechRecognitionResult;
}
interface SpeechRecognitionEventLike extends Event {
  readonly resultIndex: number;
  readonly results: SpeechRecognitionResultList;
}
interface SpeechRecognitionLike extends EventTarget {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  onresult: ((e: SpeechRecognitionEventLike) => void) | null;
  onend: (() => void) | null;
  onerror: (() => void) | null;
  start(): void;
  stop(): void;
}
interface SpeechRecognitionCtor {
  new (): SpeechRecognitionLike;
}

// SpeechRecognition is prefixed in some browsers
const SpeechRecognitionImpl =
  (typeof window !== "undefined" &&
    ((window as any).SpeechRecognition || (window as any).webkitSpeechRecognition)) as
    SpeechRecognitionCtor | undefined;

export const isVoiceSupported = !!SpeechRecognitionImpl;

interface UseVoiceChatOptions {
  onTranscript: (text: string) => void;
}

export function useVoiceChat({ onTranscript }: UseVoiceChatOptions) {
  const [isVoiceMode, setIsVoiceMode] = useState(false);
  const [isListening, setIsListening] = useState(false);
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [transcript, setTranscript] = useState("");

  const recognitionRef = useRef<SpeechRecognitionLike | null>(null);
  const onTranscriptRef = useRef(onTranscript);
  onTranscriptRef.current = onTranscript;

  const stopListening = useCallback(() => {
    recognitionRef.current?.stop();
    setIsListening(false);
  }, []);

  const startListening = useCallback(() => {
    if (!SpeechRecognitionImpl || !isVoiceMode) return;

    const rec = new SpeechRecognitionImpl();
    rec.continuous = false;
    rec.interimResults = true;
    rec.lang = "en-US";

    rec.onresult = (e: SpeechRecognitionEventLike) => {
      let interim = "";
      let final = "";
      for (let i = e.resultIndex; i < e.results.length; i++) {
        const t = e.results[i][0].transcript;
        if (e.results[i].isFinal) {
          final += t;
        } else {
          interim += t;
        }
      }
      setTranscript(final || interim);
      if (final) {
        onTranscriptRef.current(final);
        setTranscript("");
      }
    };

    rec.onend = () => setIsListening(false);
    rec.onerror = () => setIsListening(false);

    recognitionRef.current = rec;
    rec.start();
    setIsListening(true);
    setTranscript("");
  }, [isVoiceMode]);

  const speak = useCallback((text: string) => {
    if (!window.speechSynthesis) return;
    // Strip code blocks — reading code aloud isn't useful
    const cleaned = text.replace(/```[\s\S]*?```/g, "(code block)").replace(/`[^`]+`/g, "");
    const utterance = new SpeechSynthesisUtterance(cleaned);
    utterance.rate = 1.1;

    utterance.onstart = () => setIsSpeaking(true);
    utterance.onend = () => {
      setIsSpeaking(false);
      // Auto-restart listening after TTS finishes
      if (isVoiceMode) startListening();
    };
    utterance.onerror = () => setIsSpeaking(false);

    window.speechSynthesis.cancel();
    window.speechSynthesis.speak(utterance);
  }, [isVoiceMode, startListening]);

  const stopSpeaking = useCallback(() => {
    window.speechSynthesis?.cancel();
    setIsSpeaking(false);
  }, []);

  const toggleVoiceMode = useCallback(() => {
    setIsVoiceMode((prev) => {
      if (prev) {
        // Turning off
        recognitionRef.current?.stop();
        window.speechSynthesis?.cancel();
        setIsListening(false);
        setIsSpeaking(false);
        setTranscript("");
        return false;
      }
      return true;
    });
  }, []);

  // Start listening when voice mode turns on
  useEffect(() => {
    if (isVoiceMode && !isListening && !isSpeaking) {
      startListening();
    }
  }, [isVoiceMode]); // eslint-disable-line react-hooks/exhaustive-deps

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      recognitionRef.current?.stop();
      window.speechSynthesis?.cancel();
    };
  }, []);

  return {
    isVoiceMode,
    isListening,
    isSpeaking,
    transcript,
    toggleVoiceMode,
    speak,
    stopSpeaking,
    stopListening,
  };
}
