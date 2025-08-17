# AgentFlow

> AI agent pipelines for Rust - Build intelligent, fault-tolerant data processing flows using the actor model

AgentFlow is a high-performance framework for building AI agent pipelines in Rust. Inspired by [pipecat-ai](https://github.com/pipecat-ai/pipecat), it provides a robust actor-based architecture for creating complex data processing workflows with built-in fault tolerance and real-time capabilities.

## 🚀 Features

- **Actor-Based Architecture**: Built on [Ractor](https://github.com/slawlor/ractor) for reliable, concurrent agent execution
- **Type-Safe Pipelines**: Strongly-typed message passing ensures correctness at compile time
- **Fault Tolerance**: Supervisor trees automatically restart failed agents without disrupting the entire pipeline
- **Real-Time Processing**: Optimized for low-latency audio, video, and text processing
- **Extensible Agents**: Rich ecosystem of pre-built agents for common AI tasks
- **Hot Reloading**: Update pipeline logic without stopping the system
- **Observability**: Built-in monitoring, metrics, and debugging tools

## 🏗️ Architecture

AgentFlow organizes processing into three core concepts:

- **Agents**: Actor-based processors that transform data (LLM agents, TTS, STT, vision models)
- **Flows**: Pipelines that connect agents together with intelligent routing
- **Observers**: Monitoring and logging components that track pipeline health

## 🎯 Use Cases

- **Voice Assistants**: STT → LLM → TTS pipelines with real-time audio processing
- **Video Analysis**: Computer vision → LLM reasoning → action generation
- **Document Processing**: OCR → text analysis → structured output generation
- **Real-time Translation**: Speech → transcription → translation → synthesis
- **Multi-modal AI**: Combining vision, language, and audio processing agents


## 🚦 Status

**Early Development** - Core architecture being built. Not yet ready for production use.

*Built with ❤️ and ⚡ for the Rust AI community*
