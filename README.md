# AgentFlow

> Async AI pipelines for Rust - Build intelligent, fault-tolerant data processing flows using the Tokio runtime

AgentFlow is a high-performance framework for building AI pipelines in Rust. Inspired by [pipecat-ai](https://github.com/pipecat-ai/pipecat), it provides a robust async architecture for creating complex data processing workflows with built-in fault tolerance and real-time capabilities. The core is powered by [Tokio](https://tokio.rs/) for scalable, concurrent execution.

## 🚀 Features

- **Async Architecture**: Built on [Tokio](https://tokio.rs/) for reliable, concurrent task execution
- **Type-Safe Pipelines**: Strongly-typed message passing ensures correctness at compile time
- **Fault Tolerance**: Supervisor trees automatically restart failed tasks without disrupting the entire pipeline
- **Real-Time Processing**: Optimized for low-latency audio, video, and text processing
- **Extensible Processors**: Rich ecosystem of pre-built processors for common AI tasks
- **Hot Reloading**: Update pipeline logic without stopping the system
- **Observability**: Built-in monitoring, metrics, and debugging tools

## 🏗️ Architecture

AgentFlow organizes processing into three core concepts:

- **Transports**: Handle input/output streams (audio, video, text)
- **Processors**: Transform and process data streams
- **Tasks**: Manage background processing and coordination

## 🎯 Use Cases

- **Voice Assistants**: STT → LLM → TTS pipelines with real-time audio processing
- **Video Analysis**: Computer vision → LLM reasoning → action generation
- **Document Processing**: OCR → text analysis → structured output generation
- **Real-time Translation**: Speech → transcription → translation → synthesis
- **Multi-modal AI**: Combining vision, language, and audio processing processors


## 🚦 Status

**Early Development** - Core architecture being built. Not yet ready for production use.

*Built with ❤️ and ⚡ for the Rust AI community*
