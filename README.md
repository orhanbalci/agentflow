# AgentFlow

> Real-time voice AI framework for Rust - Build intelligent conversational agents with low-latency audio processing

AgentFlow is a high-performance framework specifically designed for building voice agents and conversational AI systems in Rust. Inspired by [pipecat-ai](https://github.com/pipecat-ai/pipecat), it provides a robust async architecture optimized for real-time audio processing, speech recognition, language model integration, and text-to-speech synthesis. The core is powered by [Tokio](https://tokio.rs/) for ultra-low latency voice interactions.

## 🚀 Features

- **Real-Time Audio Processing**: Optimized for low-latency voice interactions with sub-100ms response times
- **Voice Pipeline Architecture**: Built on [Tokio](https://tokio.rs/) for concurrent audio stream processing
- **Speech Integration**: Native support for STT (Speech-to-Text) and TTS (Text-to-Speech) engines
- **Conversational AI**: Seamless integration with LLMs for intelligent voice responses
- **Audio Transport Layer**: High-performance audio input/output with multiple codec support
- **Voice Activity Detection**: Smart silence detection and audio segmentation
- **Interrupt Handling**: Natural conversation flow with barge-in and interruption support
- **Hot Reloading**: Update voice models and pipeline logic without dropping audio streams
- **Voice Metrics**: Real-time monitoring of latency, quality, and conversation metrics

## 🏗️ Architecture

AgentFlow organizes voice processing into four core concepts optimized for conversational AI:

- **Voice Pipelines**: Connect STT → LLM → TTS processors in real-time audio flows
- **Audio Transports**: Handle microphone input, speaker output, and network audio streams
- **Voice Processors**: Transform speech, text, and audio data with specialized voice components
- **Conversation Tasks**: Manage dialogue state, turn-taking, and background voice processing

### Voice Pipeline System

AgentFlow's pipeline system is specifically designed for voice agent workflows:

```rust
use agentflow::pipeline::{BasePipeline, Pipeline};
use agentflow::processors::FrameProcessor;

// Create a voice agent pipeline: Audio → STT → LLM → TTS → Audio
let voice_pipeline = Pipeline::with_processors(
    "VoiceAgent".to_string(),
    vec![stt_processor, llm_processor, tts_processor],
    task_manager,
);

// Process audio frames through the voice pipeline
voice_pipeline.process_frame(audio_frame, FrameDirection::Downstream).await?;
```

**Voice Pipeline Features:**
- 🎤 **Audio Processing**: Real-time microphone input and speaker output handling
- 🗣️ **Speech Recognition**: Streaming STT with partial results and voice activity detection
- 🧠 **LLM Integration**: Low-latency language model processing for conversational responses
- � **Speech Synthesis**: High-quality TTS with emotion and voice cloning support
- 🔄 **Bidirectional Audio**: Support for full-duplex conversation with interrupt handling
- 📊 **Voice Metrics**: Latency tracking, audio quality monitoring, and conversation analytics

## 🎯 Use Cases

- **Voice Assistants**: Build Alexa/Google Assistant-style conversational agents
- **Customer Service Bots**: Automated phone support with natural conversation flow
- **Voice-Controlled Applications**: Add voice interfaces to desktop and mobile apps
- **Live Translation**: Real-time speech translation with voice preservation
- **Interactive Voice Response (IVR)**: Modern IVR systems with AI-powered understanding
- **Voice Cloning & Synthesis**: Personal voice assistants with custom voice models
- **Accessibility Tools**: Voice-controlled interfaces for users with disabilities
- **Gaming NPCs**: Intelligent voice-enabled non-player characters
- **Voice Biometrics**: Speaker identification and voice authentication systems


## 🚦 Status

**Early Development** - Core voice processing architecture being built. Audio transport layer and STT/TTS integrations in progress. Not yet ready for production voice agents.

*Built with ❤️ and 🎤 for the Rust voice AI community*
