# Crush

Crush is a Rust-based port of pigz that adds support for hardware acceleration and a plugin architecture for file-format-specific compression extensions.

It is designed for high-throughput data pipelines, particularly in AI and ML workflows where large datasets are frequently ingested from internet sources. Fast compression enables files to be compressed quickly, transferred efficiently over the network, and decompressed with minimal overhead, reducing end-to-end data ingestion time.

## Why Crush?

Modern AI and ML pipelines move massive amounts of data, often across networks and from untrusted or bandwidth-constrained sources. In these environments, compression speed matters just as much as compression ratio.

Crush is built to:

- Maximize throughput using parallel compression and hardware acceleration
- Reduce network transfer time by compressing data as early as possible in the pipeline
- Adapt to data formats through a plugin model that enables format-aware compression strategies
- Integrate cleanly with Rust-based systems, offering safety, performance, and predictable behavior

By focusing on fast, extensible compression rather than one-size-fits-all algorithms, Crush helps data pipelines move faster without becoming a bottleneck.

## Contributing

See the [Contributing Guide](CONTRIBUTING.md) for details on how to contribute to Crush.
