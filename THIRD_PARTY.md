# Сторонние компоненты

Лицензии зависимостей для macOS и Windows из Cargo.lock. GPUI-каталог обновлен по normal dependency graph из `cargo metadata --locked --filter-platform` для aarch64-apple-darwin и x86_64-pc-windows-msvc; прежние записи legacy UI сохранены как superset. Тексты с атрибуцией включены в THIRD_PARTY_LICENSES.txt. Системные frameworks поставляются ОС.

| Crate | Версия | Лицензия | Тексты |
|---|---|---|---|
| ab_glyph | 0.2.32 | Apache-2.0 | a6cba85bc92e |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | a6cba85bc92e |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | 861399f8c21c, 8ada45cd9f84, 23f18e03dc49 |
| ahash | 0.8.12 | MIT OR Apache-2.0 | a60eea817514, 0444c6991eea |
| aho-corasick | 1.1.5 | Unlicense OR MIT | gpui-01c266bced4a, gpui-0f96a83840e1 |
| aligned | 0.4.3 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-035e70219855 |
| aligned-vec | 0.6.4 | MIT | gpui-bf5a93d10144 |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| arboard | 3.7.0 | MIT OR Apache-2.0 | 0d542e0c8804, f35468024534 |
| arg_enum_proc_macro | 0.3.4 | MIT | gpui-8eb4fb5a6505 |
| arrayref | 0.3.9 | BSD-2-Clause | gpui-1bc7e6f475b3 |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | a60eea817514, 4da95ec4ecb6 |
| as-slice | 0.2.1 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-c986bcdb8310 |
| async-channel | 1.9.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-compression | 0.4.52 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-88d1e3160df4 |
| async-executor | 1.14.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-fs | 2.2.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-global-executor | 2.4.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-io | 2.6.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-lock | 3.5.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-net | 2.0.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-process | 2.5.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-std | 1.13.2 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async-task | 4.7.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| async_zip | 0.0.17 | MIT | gpui-a9fb71eab5d2 |
| atomic | 0.5.3 | Apache-2.0/MIT | gpui-a60eea817514, gpui-c9a75f18b9ab |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | a60eea817514, 23f18e03dc49, 6226d0632e2e |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | a60eea817514, 27995d58ad5c |
| av-scenechange | 0.14.1 | MIT | gpui-1e27e28eafbc |
| av1-grain | 0.2.5 | BSD-2-Clause | gpui-acc9e236b001 |
| avif-serialize | 0.8.9 | BSD-3-Clause | gpui-04556ac17ea7 |
| base64 | 0.22.1 | MIT OR Apache-2.0 | a60eea817514, 0dd882e53de1 |
| base64 | 0.23.1 | MIT OR Apache-2.0 | a60eea817514, ab499c75a0f0 |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | 8173d5c29b4f, f51ac2c59a22 |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | 8173d5c29b4f, f51ac2c59a22 |
| bit_field | 0.10.4 | MIT OR Apache-2.0 | gpui-b40930bbcf80, gpui-2346961b4509 |
| bitflags | 1.3.2 | MIT/Apache-2.0 | a60eea817514, 6485b8ed310d |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | a60eea817514, 6485b8ed310d |
| bitstream-io | 4.10.0 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-7a65c85c057a |
| block | 0.1.6 | MIT | SPDX MIT terms; manifest attribution |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | a9040321c371, d5c22aa3118d |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | gpui-a9040321c371, gpui-98181e7249d0 |
| block2 | 0.5.1 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| block2 | 0.6.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| blocking | 1.7.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| bstr | 1.13.1 | MIT OR Apache-2.0 | gpui-68653aaa727a, gpui-a60eea817514, gpui-6b7374c39a57 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-65f94e99ddaf |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | e3ba223bb142, 9df9ba60a11a, 84b34dd7608f |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | e3ba223bb142, 9df9ba60a11a, 84b34dd7608f |
| byteorder | 1.5.0 | Unlicense OR MIT | 01c266bced4a, 0f96a83840e1 |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | 0f96a83840e1 |
| bytes | 1.12.1 | MIT | 45f522cacecb |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | a60eea817514, 378f5840b258 |
| cfg_aliases | 0.2.2 | MIT | 31b94860253d, 1e2b7ade3fb2 |
| cgl | 0.3.2 | MIT / Apache-2.0 | ec82b96487e9, a60eea817514, 62065228e42c |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | gpui-a9040321c371, gpui-b8c6939380a4 |
| chrono | 0.4.45 | MIT OR Apache-2.0 | 946c9835d803 |
| clipboard-win | 5.4.3 | BSL-1.0 | c9bff7573892 |
| cocoa | 0.26.0 | MIT OR Apache-2.0 | gpui-ec82b96487e9, gpui-a60eea817514, gpui-62065228e42c |
| cocoa-foundation | 0.2.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-62065228e42c |
| codespan-reporting | 0.12.0 | Apache-2.0 | c71d239df917 |
| color_quant | 1.1.0 | MIT | gpui-592dc80f1a86 |
| command-fds | 0.3.3 | Apache-2.0 | gpui-c71d239df917 |
| compression-codecs | 0.4.47 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-88d1e3160df4 |
| compression-core | 0.4.34 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-88d1e3160df4 |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| const-oid | 0.10.3 | Apache-2.0 OR MIT | gpui-a9040321c371, gpui-73b9dc2e79c7 |
| convert_case | 0.4.0 | MIT | gpui-f9c4c77baa38 |
| core-foundation | 0.10.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-62065228e42c |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | a60eea817514, 62065228e42c |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | a60eea817514, 62065228e42c |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | a60eea817514, 62065228e42c |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 | ec82b96487e9, a60eea817514, 62065228e42c |
| core-graphics | 0.24.0 | MIT OR Apache-2.0 | gpui-ec82b96487e9, gpui-a60eea817514, gpui-62065228e42c |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 | a60eea817514, 62065228e42c |
| core-graphics-types | 0.2.0 | MIT OR Apache-2.0 | a60eea817514, 62065228e42c |
| core-graphics2 | 0.4.1 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-4646dda6399f |
| core-text | 21.0.0 | MIT OR Apache-2.0 | gpui-ec82b96487e9, gpui-a60eea817514, gpui-62065228e42c |
| core-video | 0.4.3 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-4646dda6399f |
| core_detect | 1.0.0 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-8569ebf6bf52 |
| core_maths | 0.1.1 | MIT | gpui-46f625f4d016 |
| coreaudio-rs | 0.13.0 | MIT/Apache-2.0 | a60eea817514, 7576269ea71f |
| cpal | 0.16.0 | Apache-2.0 | c71d239df917 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | a9040321c371, ae9baa7beea9 |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | gpui-a9040321c371, gpui-73b9dc2e79c7 |
| crc32fast | 1.5.4 | MIT OR Apache-2.0 | c6596eb7be85, 61d383b05b87 |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | a60eea817514, 5734ed989dfc, b16db96b93b1 |
| crossbeam-deque | 0.8.8 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-5734ed989dfc |
| crossbeam-epoch | 0.9.21 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-5734ed989dfc |
| crossbeam-queue | 0.3.14 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-5734ed989dfc |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | a60eea817514, 5734ed989dfc |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | a9040321c371, 3521672491a3 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | gpui-a9040321c371, gpui-d2e7ec5355c9 |
| ctor | 0.4.3 | Apache-2.0 OR MIT | gpui-a8ad31b1c3f4, gpui-fd80a26fbb3f |
| ctor-proc-macro | 0.0.6 | Apache-2.0 OR MIT | gpui-a8ad31b1c3f4, gpui-fd80a26fbb3f |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | b59affd015e5, c138726a726a, 01e533bd94e8 |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 | 756c8d2ab2dc, b1d6df41ed3a |
| data-url | 0.3.2 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-b38f11f60967 |
| deflate64 | 0.1.12 | MIT | gpui-86b5aedec040 |
| derive_more | 0.99.20 | MIT | gpui-8a35369f3ca2 |
| digest | 0.10.7 | MIT OR Apache-2.0 | a9040321c371, 9e0dfd2dd417 |
| digest | 0.11.3 | MIT OR Apache-2.0 | gpui-a9040321c371, gpui-af59cea35d7f |
| directories | 6.0.0 | MIT OR Apache-2.0 | d3174ad63e72, 11955c617c19 |
| dirs | 4.0.0 | MIT OR Apache-2.0 | gpui-d3174ad63e72, gpui-6a2e0ade09a7 |
| dirs | 5.0.1 | MIT OR Apache-2.0 | gpui-d3174ad63e72, gpui-6a2e0ade09a7 |
| dirs-sys | 0.3.7 | MIT OR Apache-2.0 | gpui-d3174ad63e72, gpui-6a2e0ade09a7 |
| dirs-sys | 0.4.1 | MIT OR Apache-2.0 | gpui-d3174ad63e72, gpui-6a2e0ade09a7 |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 | d3174ad63e72, 6a2e0ade09a7 |
| discord-rich-presence | 1.1.0 | MIT | 28e9aeba487f |
| dispatch | 0.2.0 | MIT | SPDX MIT terms; manifest attribution |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | a60eea817514, 23f18e03dc49 |
| dlib | 0.5.3 | MIT | gpui-7bbe6f97e377 |
| document-features | 0.2.12 | MIT OR Apache-2.0 | 074e6e32c86a, aa893340d14b |
| dpi | 0.1.2 | Apache-2.0 AND MIT | 6dc0e068dcf3, 9eff396a99ba |
| dtor | 0.0.6 | Apache-2.0 OR MIT | gpui-752daf2fb234 |
| dtor-proc-macro | 0.0.5 | Apache-2.0 OR MIT | gpui-752daf2fb234 |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | gpui-a2010f343487 |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| ecolor | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| eframe | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| egui | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| egui-wgpu | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| egui-winit | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| egui_glow | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| either | 1.19.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-7576269ea71f |
| emath | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| encoding_rs | 0.8.44 | (Apache-2.0 OR MIT) AND BSD-3-Clause | gpui-11789f45bb18, gpui-cfc7749b96f6, gpui-3fa4ca83dcc9, gpui-838118388fe5 |
| epaint | 0.33.3 | MIT OR Apache-2.0 | 8173d5c29b4f, 95ca92f5f8ea |
| epaint_default_fonts | 0.33.3 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | 6a73f9541c2d, b9d2c1d909aa |
| equator | 0.4.2 | MIT | gpui-97d3424e347e |
| equator-macro | 0.4.2 | MIT | gpui-97d3424e347e |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | a60eea817514, 7365cc8878a1 |
| erased-serde | 0.4.11 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| errno | 0.3.14 | MIT OR Apache-2.0 | a60eea817514, 8764a5976757 |
| error-code | 3.4.0 | BSL-1.0 | c9bff7573892 |
| etagere | 0.2.15 | MIT/Apache-2.0 | gpui-2d56a37f3d1e, gpui-9cfc9c9a4608 |
| euclid | 0.22.14 | MIT OR Apache-2.0 | gpui-ec82b96487e9, gpui-a60eea817514, gpui-62065228e42c |
| event-listener | 2.5.3 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| exr | 1.74.3 | BSD-3-Clause | gpui-fdd721cf424c |
| fastrand | 1.9.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | a60eea817514, 23f18e03dc49 |
| fax | 0.2.7 | MIT | 7101390b4c8e |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | 0d542e0c8804, c77a4cf9da72 |
| filetime | 0.2.29 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-378f5840b258 |
| flate2 | 1.1.11 | MIT OR Apache-2.0 | a60eea817514, 025436edff4c |
| float-cmp | 0.9.0 | MIT | gpui-40be1e77825d |
| float-ord | 0.3.2 | MIT / Apache-2.0 | gpui-b40930bbcf80, gpui-0a371b88c0f6 |
| float_next_after | 1.0.0 | MIT | gpui-0604d5817110 |
| flume | 0.11.1 | Apache-2.0/MIT | gpui-e48c2570866e, gpui-23f18e03dc49 |
| fnv | 1.0.7 | Apache-2.0 / MIT | gpui-a60eea817514, gpui-65fdb6c76cd6 |
| foldhash | 0.2.0 | Zlib | b1181a40b2a7 |
| fontdb | 0.23.0 | MIT | gpui-59a1fdac3bd5 |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | c6596eb7be85, 333ea3aaa3ca |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | c6596eb7be85, 333ea3aaa3ca |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | c6596eb7be85, 333ea3aaa3ca |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | a60eea817514, 20c7855c364d |
| futf | 0.1.5 | MIT / Apache-2.0 | gpui-a60eea817514, gpui-60a7062291b0 |
| futures | 0.3.34 | MIT OR Apache-2.0 | gpui-275c491d6d11, gpui-6652c868f35d |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | 275c491d6d11, 6652c868f35d |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | 275c491d6d11, 6652c868f35d |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | gpui-275c491d6d11, gpui-6652c868f35d |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | 275c491d6d11, 6652c868f35d |
| futures-lite | 1.13.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49, gpui-6226d0632e2e |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49, gpui-6226d0632e2e |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | gpui-275c491d6d11, gpui-6652c868f35d |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | 275c491d6d11, 6652c868f35d |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | 275c491d6d11, 6652c868f35d |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | 275c491d6d11, 6652c868f35d |
| generic-array | 0.14.7 | MIT | ad4fcfaf8d5b |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | aaff376532ea, 42fa16951ce7 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | aaff376532ea, 29e9fe5074bd |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | aaff376532ea, 523a42c25d24 |
| gif | 0.14.2 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-77257f3d2181 |
| gl_generator | 0.14.0 | Apache-2.0 | cfc7749b96f6 |
| globset | 0.4.21 | Unlicense OR MIT | gpui-01c266bced4a, gpui-0f96a83840e1 |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib | 769f80b5bcb4, 30fefc3a7d6a, b3ec01ad0869 |
| glutin | 0.32.3 | Apache-2.0 | a44454274400 |
| glutin-winit | 0.5.0 | MIT | d35629aec828 |
| glutin_egl_sys | 0.7.1 | Apache-2.0 | a44454274400 |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | a44454274400 |
| gpu-allocator | 0.27.0 | MIT OR Apache-2.0 | 0178e21322b0, ad41be6cc653 |
| gpui | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui-macros | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_collections | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_derive_refineable | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_http_client | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_media | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_perf | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_refineable | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_semantic_version | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_sum_tree | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_util | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| gpui_util_macros | 0.2.2 | Apache-2.0 | gpui-752daf2fb234 |
| grid | 0.18.0 | MIT | gpui-addb019eb802 |
| h2 | 0.4.20 | MIT | gpui-b21623012e6c |
| half | 2.7.1 | MIT OR Apache-2.0 | a6cba85bc92e, 508a77d2e7b5 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | a60eea817514, ff8f68cb076c |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | a60eea817514, ff8f68cb076c |
| heck | 0.5.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-7b63ecd5f190 |
| hexf-parse | 0.2.1 | CC0-1.0 | 3662a9adfe6f |
| home | 0.5.12 | MIT OR Apache-2.0 | gpui-8ada45cd9f84, gpui-23f18e03dc49 |
| http | 1.5.0 | MIT OR Apache-2.0 | 8bb1b50b0e5c, dc91f8200e4b |
| http-body | 1.1.0 | MIT | 248378d0a338 |
| http-body-util | 0.1.5 | MIT | 248378d0a338 |
| httparse | 1.10.1 | MIT OR Apache-2.0 | a60eea817514, 391a5396cec6 |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | gpui-a9040321c371, gpui-70c9d40f1f95 |
| hyper | 1.12.0 | MIT | 2d0189041449 |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | gpui-a60eea817514, gpui-7cfafc877ecc, gpui-709e3175b421 |
| hyper-tls | 0.6.0 | MIT/Apache-2.0 | a60eea817514, 7b033f57f23f |
| hyper-util | 0.1.21 | MIT | 9e0a97848ea5 |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | 696759d65dfe, da28ccc6b158 |
| icu_collections | 2.4.0 | Unicode-3.0 | f367c1b8e1aa |
| icu_locale_core | 2.4.0 | Unicode-3.0 | f367c1b8e1aa |
| icu_normalizer | 2.4.0 | Unicode-3.0 | f367c1b8e1aa |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | f367c1b8e1aa |
| icu_properties | 2.4.0 | Unicode-3.0 | f367c1b8e1aa |
| icu_properties_data | 2.4.0 | Unicode-3.0 | f367c1b8e1aa |
| icu_provider | 2.4.0 | Unicode-3.0 | f367c1b8e1aa |
| idna | 1.1.0 | MIT OR Apache-2.0 | a60eea817514, b38f11f60967 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | a60eea817514, 8b43ce8accd6 |
| image | 0.25.10 | MIT OR Apache-2.0 | 0d542e0c8804, c77a4cf9da72 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | gpui-0d542e0c8804, gpui-c77a4cf9da72 |
| imagesize | 0.13.0 | MIT | gpui-fa0d694967d5 |
| imgref | 1.12.3 | CC0-1.0 OR Apache-2.0 | gpui-c71d239df917, gpui-a2010f343487 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | a60eea817514, ecc269ef87fd |
| inventory | 0.3.25 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| io-surface | 0.16.1 | MIT OR Apache-2.0 | gpui-ec82b96487e9, gpui-a60eea817514, gpui-62065228e42c |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | 87d9feb9238c, 47dc9ff29128 |
| itertools | 0.14.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-7576269ea71f |
| itoa | 1.0.19 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| keyring | 3.6.3 | MIT OR Apache-2.0 | 78ebc2ea2a1c, 157cc607717b |
| khronos_api | 3.1.0 | Apache-2.0 | cfc7749b96f6 |
| kurbo | 0.11.3 | Apache-2.0 OR MIT | gpui-cfc7749b96f6, gpui-23c23145f6ea |
| kv-log-macro | 1.0.7 | MIT OR Apache-2.0 | gpui-b00ec75cd7ef, gpui-ce688e08d045 |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-23f18e03dc49 |
| leak | 0.1.2 | Apache-2.0 OR MIT | gpui-752daf2fb234 |
| leaky-cow | 0.1.1 | MIT / Apache-2.0 | gpui-752daf2fb234 |
| lebe | 0.5.3 | BSD-3-Clause | gpui-e5d7cead4e81 |
| libc | 0.2.191 | MIT OR Apache-2.0 | 62c7a1e35f56, 123a331b5dbf |
| libloading | 0.8.9 | ISC | b29f8b014523 |
| libm | 0.2.16 | MIT | 3823dda7cf04 |
| litemap | 0.8.3 | Unicode-3.0 | f367c1b8e1aa |
| litrs | 1.0.1 | MIT OR Apache-2.0 | 62c7a1e35f56, 7dc1552e88f4 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | a60eea817514, c9a75f18b9ab |
| log | 0.4.35 | MIT OR Apache-2.0 | a60eea817514, 6485b8ed310d |
| loop9 | 0.1.5 | MIT | gpui-af8a1cdcbe55 |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib | gpui-cfc7749b96f6, gpui-1466d9b4f3a5, gpui-09785877275f |
| lyon | 1.0.19 | MIT OR Apache-2.0 | gpui-216486f29671 |
| lyon_algorithms | 1.0.22 | MIT OR Apache-2.0 | gpui-216486f29671 |
| lyon_geom | 1.0.20 | MIT OR Apache-2.0 | gpui-216486f29671 |
| lyon_path | 1.0.19 | MIT OR Apache-2.0 | gpui-216486f29671 |
| lyon_tessellation | 1.0.23 | MIT OR Apache-2.0 | gpui-216486f29671 |
| mac | 0.1.1 | MIT/Apache-2.0 | gpui-752daf2fb234 |
| mach2 | 0.4.3 | BSD-2-Clause OR MIT OR Apache-2.0 | 62c7a1e35f56, 044983df14c9, 3f9f0f7e5a59 |
| malloc_buf | 0.0.6 | MIT | aced7b57e474 |
| maybe-rayon | 0.1.1 | MIT | gpui-3b2dbf868450 |
| memchr | 2.8.3 | Unlicense OR MIT | 01c266bced4a, 0f96a83840e1 |
| memmap2 | 0.9.12 | MIT OR Apache-2.0 | gpui-04ea4849dba9, gpui-0d25d03b5ab4 |
| memoffset | 0.9.1 | MIT | 3234ac558162 |
| metal | 0.29.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-0621878e61f0 |
| metal | 0.32.0 | MIT OR Apache-2.0 | a60eea817514, 0621878e61f0 |
| mime | 0.3.18 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-8b87502eddb2 |
| mime_guess | 2.0.5 | MIT | gpui-14373973f98d |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | 4108245a1f2d, 0d542e0c8804, 799e9ca9d179, 0a54e647fe54 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | 4108245a1f2d, 0d542e0c8804, 799e9ca9d179, 0a54e647fe54 |
| mio | 1.2.4 | MIT | 07919255c7e0 |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | 90bf2d659c43, 2aa92cada643 |
| multiversion | 0.8.1 | MIT OR Apache-2.0 | gpui-cfc7749b96f6, gpui-3c92a19e5417 |
| multiversion-macros | 0.8.1 | MIT OR Apache-2.0 | gpui-cfc7749b96f6, gpui-3c92a19e5417 |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT | gpui-d2a809eafabb, gpui-cfc7749b96f6, gpui-3fa4ca83dcc9 |
| naga | 27.0.3 | MIT OR Apache-2.0 | a6cba85bc92e, dc0d97139e82 |
| nanorand | 0.7.0 | Zlib | gpui-ad4d94921cc5 |
| native-tls | 0.2.18 | MIT OR Apache-2.0 | c6596eb7be85, f2ad7982ddbf |
| new_debug_unreachable | 1.0.6 | MIT | gpui-f7715d38a3fa |
| nix | 0.29.0 | MIT | gpui-66e3ee1fa7f9 |
| nix | 0.31.3 | MIT | gpui-66e3ee1fa7f9 |
| no_std_io2 | 0.9.4 | Apache-2.0 OR MIT | gpui-b40930bbcf80, gpui-f21c3bfdc477 |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | cfc7749b96f6, ec353d4fecf7 |
| nom | 8.0.0 | MIT | gpui-4dbda0434445 |
| noop_proc_macro | 0.3.0 | MIT | gpui-d69f24ad84ec |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-6485b8ed310d |
| num-derive | 0.4.2 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-6485b8ed310d |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-6485b8ed310d |
| num-rational | 0.4.2 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-6485b8ed310d |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | a60eea817514, 6485b8ed310d |
| num_cpus | 1.17.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-1626f2c950ce |
| oauth2 | 5.0.0 | MIT OR Apache-2.0 | a60eea817514, 378f5840b258 |
| objc | 0.2.7 | MIT | e353f37b12ae |
| objc-sys | 0.3.5 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2 | 0.5.3 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2 | 0.6.5 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-app-kit | 0.2.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-audio-toolbox | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-av-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-avf-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-audio-types | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-data | 0.2.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-image | 0.2.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-image | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-media | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-core-video | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-encode | 4.1.0 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-foundation | 0.2.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-foundation | 0.3.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-image-io | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-media-toolbox | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-metal | 0.2.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-quartz-core | 0.2.2 | MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | 0c9a5f6fe28e, f6a2e783669a, fced3e7fedc2, e489d07e5496 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | a60eea817514, 23f18e03dc49 |
| option-ext | 0.2.0 | MPL-2.0 | 66a3107d5ad6 |
| ordered-float | 5.5.1 | MIT | f7715d38a3fa |
| owned_ttf_parser | 0.25.1 | Apache-2.0 | a6cba85bc92e |
| parking | 2.2.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49, gpui-9a6d7a3c1b8e |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | a60eea817514, c9a75f18b9ab |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | a60eea817514, c9a75f18b9ab |
| paste | 1.0.15 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| pastey | 0.1.1 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| pathfinder_geometry | 0.5.1 | MIT/Apache-2.0 | gpui-752daf2fb234 |
| pathfinder_simd | 0.5.6 | MIT OR Apache-2.0 | gpui-a60eea817514 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | a60eea817514, b38f11f60967 |
| pico-args | 0.5.0 | MIT | gpui-be8e392dd419 |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | gpui-0d542e0c8804, gpui-23f18e03dc49 |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | gpui-0d542e0c8804, gpui-23f18e03dc49 |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | 0d542e0c8804, 23f18e03dc49 |
| pin-utils | 0.1.1 | MIT OR Apache-2.0 | gpui-a4db788775cb, gpui-28802412d2bf |
| piper | 0.2.5 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-23f18e03dc49 |
| png | 0.17.16 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-eaf40297c75d |
| png | 0.18.1 | MIT OR Apache-2.0 | a60eea817514, eaf40297c75d |
| polling | 3.11.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| pollster | 0.2.5 | Apache-2.0/MIT | gpui-a60eea817514, gpui-cc02a5331667 |
| pollster | 0.4.0 | Apache-2.0/MIT | a60eea817514, cc02a5331667 |
| postage | 0.5.0 | MIT | gpui-d4a0afb0c4ac |
| potential_utf | 0.1.6 | Unicode-3.0 | f367c1b8e1aa |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | 0218327e7a48, 4cada0bd02ea |
| presser | 0.3.1 | MIT OR Apache-2.0 | 8173d5c29b4f, 090a294a492a |
| proc-macro-error-attr2 | 2.0.0 | MIT OR Apache-2.0 | gpui-6fd0f3522047, gpui-544b3aed1fd7 |
| proc-macro-error2 | 2.0.1 | MIT OR Apache-2.0 | gpui-6fd0f3522047, gpui-544b3aed1fd7 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| profiling | 1.0.18 | MIT OR Apache-2.0 | 10d30a673cd5, c8167fdeeed4 |
| profiling-procmacros | 1.0.18 | MIT OR Apache-2.0 | gpui-6ee32466aaa7 |
| psm | 0.1.32 | MIT OR Apache-2.0 | gpui-965a63a81d9a, gpui-3e3714aa69bd |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | 90bf2d659c43, 2aa92cada643 |
| qoi | 0.4.1 | MIT/Apache-2.0 | gpui-8173d5c29b4f, gpui-45adbcb6d50e |
| quick-error | 2.0.1 | MIT/Apache-2.0 | c6596eb7be85, 058f01fe1816 |
| quinn | 0.11.12 | MIT OR Apache-2.0 | gpui-c71d239df917, gpui-4b2d0aca6789 |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 | gpui-c71d239df917, gpui-4b2d0aca6789 |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 | gpui-c71d239df917, gpui-4b2d0aca6789 |
| quote | 1.0.47 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| rand | 0.10.3 | MIT OR Apache-2.0 | gpui-90eb64f0279b, gpui-35242e7a83f6, gpui-209fbbe0ad52 |
| rand | 0.8.8 | MIT OR Apache-2.0 | 90eb64f0279b, 35242e7a83f6, 209fbbe0ad52 |
| rand | 0.9.5 | MIT OR Apache-2.0 | gpui-90eb64f0279b, gpui-35242e7a83f6, gpui-209fbbe0ad52 |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 | 90eb64f0279b, aaff376532ea, 209fbbe0ad52 |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | gpui-90eb64f0279b, gpui-35242e7a83f6, gpui-209fbbe0ad52 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | gpui-92b81db30f7a, gpui-6df43f6f4b5d, gpui-8b6e9feec03e |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | 90eb64f0279b, 6df43f6f4b5d, 209fbbe0ad52 |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | gpui-90eb64f0279b, gpui-6df43f6f4b5d, gpui-209fbbe0ad52 |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | gpui-90eb64f0279b, gpui-6df43f6f4b5d, gpui-2234e3cefee8 |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 | a6cba85bc92e, ba00052102ed |
| rav1e | 0.8.1 | BSD-2-Clause | gpui-2dd0a109751e |
| ravif | 0.13.0 | BSD-3-Clause | gpui-f78709143bb9 |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | 0d542e0c8804, 9c5a80639a57, ae895c8576e6 |
| rayon | 1.12.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-0621878e61f0 |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-0621878e61f0 |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| regex | 1.13.1 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-6485b8ed310d |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-6485b8ed310d |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-6485b8ed310d |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | a60eea817514, 24b2f2eff5a7 |
| reqwest | 0.12.28 | MIT OR Apache-2.0 | 751963a8b88c, 47d4e1803702 |
| resvg | 0.45.1 | Apache-2.0 OR MIT | gpui-a6cba85bc92e, gpui-f5d934dc281b |
| rgb | 0.8.53 | MIT | gpui-457b464bc0fb |
| ring | 0.17.14 | Apache-2.0 AND ISC | gpui-b3d734001a94, gpui-005fc765ddc5, gpui-f025ccfb7dfb |
| roxmltree | 0.20.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-fffe218c1e34 |
| rust-embed | 8.14.0 | MIT | gpui-57f956f28566 |
| rust-embed-impl | 8.14.0 | MIT | gpui-57f956f28566 |
| rust-embed-utils | 8.14.0 | MIT | gpui-57f956f28566 |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | a60eea817514, 23f18e03dc49 |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | 95bd3988beee, 30fefc3a7d6a |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | gpui-377c2e7c5325, gpui-a60eea817514, gpui-a6c48161a09a, gpui-23f18e03dc49 |
| rustix | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 377c2e7c5325, a60eea817514, 268872b9816f, 23f18e03dc49 |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | gpui-a60eea817514, gpui-7cfafc877ecc, gpui-709e3175b421 |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT | gpui-2775d45acac7, gpui-a60eea817514, gpui-7cfafc877ecc, gpui-709e3175b421 |
| rustls-pemfile | 2.2.0 | Apache-2.0 OR ISC OR MIT | gpui-e1da6bc3092f, gpui-a60eea817514, gpui-7cfafc877ecc, gpui-709e3175b421 |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | 45fd05c4865e, 9117d922e667 |
| rustls-webpki | 0.103.17 | ISC | gpui-5b698ca13897 |
| rustybuzz | 0.20.1 | MIT | gpui-3a7c3f0b887a |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | 62c7a1e35f56, c9bff7573892 |
| same-file | 1.0.6 | Unlicense/MIT | gpui-01c266bced4a, gpui-cb3c929a05e6 |
| schannel | 0.1.30 | MIT | aa72991ac35b |
| schemars | 1.2.3 | MIT | gpui-1954992a2b32 |
| schemars_derive | 1.2.3 | MIT | gpui-1954992a2b32 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | a60eea817514, fb77f0a9c53e |
| seahash | 4.1.0 | MIT | gpui-f9c4c77baa38 |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | a60eea817514, 91e934255ba3 |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | a60eea817514, 91e934255ba3 |
| serde | 1.0.229 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| serde_fmt | 1.1.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-782a2a0d5242 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| serde_json_lenient | 0.2.4 | MIT/Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | 62c7a1e35f56, b9eb26629432 |
| sha1_smol | 1.0.1 | BSD-3-Clause | gpui-282a21b07104 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | a9040321c371, b4eb00df6e2a |
| sha2 | 0.11.1 | MIT OR Apache-2.0 | gpui-a9040321c371, gpui-831e0f43ad0b |
| shlex | 1.3.0 | MIT OR Apache-2.0 | gpui-553fffcd9b1c, gpui-4455bf75a911 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-503558bfefe6 |
| simd-adler32 | 0.3.10 | MIT | 42a35170233e |
| simd_helpers | 0.1.0 | MIT | gpui-f9c4c77baa38 |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | gpui-0d542e0c8804, gpui-508a77d2e7b5 |
| simplecss | 0.2.2 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-0b5f24524360 |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | gpui-c962ee4d1d05, gpui-cfc7749b96f6, gpui-f9c4c77baa38 |
| slab | 0.4.12 | MIT | 8ce0830173fd |
| slotmap | 1.1.1 | Zlib | gpui-6cec81441b2a |
| smallvec | 1.16.5 | MIT OR Apache-2.0 | a60eea817514, 0b28172679e0 |
| smol | 2.0.2 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | a60eea817514, 23f18e03dc49 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | a60eea817514, 378f5840b258 |
| spin | 0.9.9 | MIT | gpui-58545fed1565 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | a60eea817514, 5e05b024f653 |
| stacker | 0.1.25 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-378f5840b258 |
| stacksafe | 0.1.4 | Apache-2.0 | gpui-c71d239df917 |
| stacksafe-macro | 0.1.4 | Apache-2.0 | gpui-c71d239df917 |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | cfc7749b96f6, ea084a2373eb |
| strict-num | 0.1.1 | MIT | gpui-3085885678bb |
| strum | 0.27.2 | MIT | gpui-8bce3b45e49e |
| strum_macros | 0.27.2 | MIT | gpui-8bce3b45e49e |
| subtle | 2.6.1 | BSD-3-Clause | gpui-cc0332a88c2e |
| sval | 2.22.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| sval_buffer | 2.22.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| sval_dynamic | 2.22.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| sval_fmt | 2.22.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| sval_json | 2.22.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| sval_nested | 2.22.0 | Apache-2.0 OR MIT | gpui-4201a739d991, gpui-71430174df61 |
| sval_ref | 2.22.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| sval_serde | 2.22.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| svg_fmt | 0.4.5 | MIT/Apache-2.0 | gpui-752daf2fb234 |
| svgtypes | 0.15.3 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-bb7d16c928ad |
| syn | 2.0.119 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| syn | 3.0.10 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| sync_wrapper | 1.0.2 | Apache-2.0 | 0d542e0c8804 |
| synstructure | 0.14.0 | MIT | 219920e865ee |
| system-configuration | 0.6.1 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-000cbb5bea6f |
| system-configuration-sys | 0.6.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-000cbb5bea6f |
| taffy | 0.9.0 | MIT | gpui-a7083ef3dd41 |
| take-until | 0.2.0 | MIT | gpui-02c755ec7c7c |
| target-features | 0.1.6 | MIT OR Apache-2.0 | gpui-cfc7749b96f6, gpui-6cfe51b21bd3 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | a60eea817514, 8b427f5bc501 |
| tendril | 0.4.3 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-60a7062291b0 |
| termcolor | 1.4.1 | Unlicense OR MIT | gpui-01c266bced4a, gpui-0f96a83840e1 |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | 62c7a1e35f56, 23f18e03dc49 |
| tiff | 0.11.3 | MIT | 20c097d0de27 |
| tiny-skia | 0.11.4 | BSD-3-Clause | gpui-6d41e05f1d54 |
| tiny-skia-path | 0.11.4 | BSD-3-Clause | gpui-6d41e05f1d54 |
| tinystr | 0.8.5 | Unicode-3.0 | f367c1b8e1aa |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | cfc7749b96f6, fd80a26fbb3f, 84b34dd7608f |
| tokio | 1.54.1 | MIT | 253cd04c6714 |
| tokio-native-tls | 0.3.1 | MIT | 898b1ae9821e |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | gpui-cc117d90b498, gpui-e20fa2b8e0a2 |
| tokio-socks | 0.5.3 | MIT | gpui-a0bb5ec9a11d |
| tokio-util | 0.7.20 | MIT | gpui-253cd04c6714 |
| tower | 0.5.3 | MIT | 4249c8e6c5eb |
| tower-http | 0.6.11 | MIT | 5049cf464977 |
| tower-layer | 0.3.3 | MIT | 4249c8e6c5eb |
| tower-service | 0.3.3 | MIT | 4249c8e6c5eb |
| tracing | 0.1.44 | MIT | 898b1ae9821e |
| tracing-attributes | 0.1.31 | MIT | gpui-898b1ae9821e |
| tracing-core | 0.1.36 | MIT | 898b1ae9821e |
| try-lock | 0.2.5 | MIT | c816a0749cdc |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | a60eea817514, f3c9fe731c70 |
| type-map | 0.5.1 | MIT/Apache-2.0 | 00fb0b04768f, 6e9587a31f8b |
| typeid | 1.0.3 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| typenum | 1.20.1 | MIT OR Apache-2.0 | db11fec99467, 516b24e051bf, a825bd853ab7 |
| unicase | 2.10.0 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-2d0189041449 |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 | gpui-edb20b474f6c, gpui-a60eea817514, gpui-7b63ecd5f190 |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-59a1fdac3bd5 |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-59a1fdac3bd5 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | 62c7a1e35f56, 23f18e03dc49, f7db81051789 |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | 23860c2a7b5d, a60eea817514, 7b63ecd5f190, f7db81051789 |
| unicode-properties | 0.1.5 | (MIT OR Apache-2.0) AND Unicode-3.0 | gpui-23860c2a7b5d, gpui-a60eea817514, gpui-7b63ecd5f190, gpui-e7a93b009565 |
| unicode-script | 0.5.9 | MIT OR Apache-2.0 | gpui-512a9e571c08, gpui-15656cc11a83 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | 23860c2a7b5d, a60eea817514, 7b63ecd5f190 |
| unicode-vo | 0.1.0 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-0b5f24524360 |
| unicode-width | 0.2.3 | MIT OR Apache-2.0 | 23860c2a7b5d, a60eea817514, 7b63ecd5f190 |
| untrusted | 0.9.0 | ISC | gpui-7abd9b6960dc |
| url | 2.5.8 | MIT OR Apache-2.0 | a60eea817514, b38f11f60967 |
| usvg | 0.45.1 | Apache-2.0 OR MIT | gpui-a6cba85bc92e, gpui-f5d934dc281b |
| utf-8 | 0.7.6 | MIT OR Apache-2.0 | gpui-62c7a1e35f56, gpui-23f18e03dc49 |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT | c30152c94a6d, cfc7749b96f6, 3fa4ca83dcc9 |
| utf_types | 1.0.0 | Apache-2.0 OR MIT | 3210be7332b5, cfc7749b96f6, 3fa4ca83dcc9 |
| uuid | 0.8.2 | Apache-2.0 OR MIT | b4b2c0de2a05, a60eea817514, 436bc5a105d8 |
| uuid | 1.29.2 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-436bc5a105d8 |
| v_frame | 0.3.9 | BSD-2-Clause | gpui-02b80951ce6c |
| value-bag | 1.14.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| value-bag-serde1 | 1.14.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| value-bag-sval2 | 1.14.1 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-5d1bf69a3993 |
| version_check | 0.9.5 | MIT/Apache-2.0 | a60eea817514, b7e650f3fce5 |
| waker-fn | 1.2.0 | Apache-2.0 OR MIT | gpui-a60eea817514, gpui-23f18e03dc49 |
| walkdir | 2.5.0 | Unlicense/MIT | gpui-01c266bced4a, gpui-0f96a83840e1 |
| want | 0.3.2 | MIT | a65f5d0a945d |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-378f5840b258 |
| wasm-bindgen-macro | 0.2.130 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-378f5840b258 |
| wasm-bindgen-macro-support | 0.2.130 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-378f5840b258 |
| wasm-bindgen-shared | 0.2.130 | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-378f5840b258 |
| web-time | 1.1.0 | MIT OR Apache-2.0 | 54a744942eb3, f428305bbf2e |
| webbrowser | 1.2.6 | MIT OR Apache-2.0 | b0f7d40eaa95, 7f0f4f8802c1 |
| weezl | 0.1.12 | MIT OR Apache-2.0 | 769f80b5bcb4, 573f5227db83 |
| wgpu | 27.0.1 | MIT OR Apache-2.0 | a6cba85bc92e, dc0d97139e82 |
| wgpu-core | 27.0.3 | MIT OR Apache-2.0 | a6cba85bc92e, dc0d97139e82 |
| wgpu-core-deps-apple | 27.0.0 | MIT OR Apache-2.0 | a6cba85bc92e, dc0d97139e82 |
| wgpu-core-deps-windows-linux-android | 27.0.0 | MIT OR Apache-2.0 | a6cba85bc92e, dc0d97139e82 |
| wgpu-hal | 27.0.4 | MIT OR Apache-2.0 | a6cba85bc92e, dc0d97139e82 |
| wgpu-types | 27.0.1 | MIT OR Apache-2.0 | a6cba85bc92e, dc0d97139e82 |
| which | 6.0.3 | MIT | gpui-0041560f5d41 |
| winapi | 0.3.9 | MIT/Apache-2.0 | gpui-b40930bbcf80, gpui-ce7bc3499fee |
| winapi-util | 0.1.11 | Unlicense OR MIT | gpui-01c266bced4a, gpui-cb3c929a05e6 |
| windows | 0.54.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows | 0.58.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows | 0.61.3 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows | 0.62.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-collections | 0.2.0 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-core | 0.54.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-core | 0.58.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-core | 0.61.2 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-future | 0.2.1 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-implement | 0.58.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-interface | 0.58.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-link | 0.1.3 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-numerics | 0.2.0 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-registry | 0.4.0 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-registry | 0.5.3 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-result | 0.1.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-result | 0.2.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-result | 0.3.4 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-strings | 0.1.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-strings | 0.3.1 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-strings | 0.4.2 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows-threading | 0.1.0 | MIT OR Apache-2.0 | gpui-c16f8dcf1a36, gpui-c2cfccb812fe |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 | c16f8dcf1a36, c2cfccb812fe |
| winit | 0.30.13 | Apache-2.0 | 6dc0e068dcf3 |
| winresource | 0.1.34 | MIT | 3564da09711c |
| winsafe | 0.0.19 | MIT | gpui-e6b9b47cd4cb |
| writeable | 0.6.4 | Unicode-3.0 | f367c1b8e1aa |
| xattr | 0.2.3 | MIT/Apache-2.0 | gpui-a60eea817514, gpui-8b427f5bc501 |
| xml-rs | 0.8.29 | MIT | 0dc18d924dc0 |
| xmlwriter | 0.1.0 | MIT | gpui-2cdce1ff4ca8 |
| y4m | 0.8.0 | MIT | gpui-1327309babc5 |
| yeslogic-fontconfig-sys | 6.0.1 | MIT | gpui-1946ace4922e |
| yoke | 0.8.4 | Unicode-3.0 | f367c1b8e1aa |
| yoke-derive | 0.8.4 | Unicode-3.0 | f367c1b8e1aa |
| zed-async-tar | 0.5.0-zed | MIT/Apache-2.0 | gpui-a60eea817514, gpui-378f5840b258 |
| zed-font-kit | 0.14.1-zed | MIT OR Apache-2.0 | gpui-a60eea817514, gpui-23f18e03dc49 |
| zed-reqwest | 0.12.15-zed | MIT OR Apache-2.0 | gpui-751963a8b88c, gpui-34f17d906738 |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | 9d185ac6703c, 83c1763356e8, 1a2f5c12ddc9 |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | 9d185ac6703c, 83c1763356e8, 1a2f5c12ddc9 |
| zerofrom | 0.1.8 | Unicode-3.0 | f367c1b8e1aa |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | f367c1b8e1aa |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | cfc7749b96f6, 8c7516d4b27b |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | gpui-cfc7749b96f6, gpui-b8c6939380a4 |
| zerotrie | 0.2.5 | Unicode-3.0 | f367c1b8e1aa |
| zerovec | 0.11.9 | Unicode-3.0 | f367c1b8e1aa |
| zerovec-derive | 0.11.7 | Unicode-3.0 | f367c1b8e1aa |
| zlib-rs | 0.6.8 | Zlib | e72111c52b7d |
| zmij | 1.0.23 | MIT | 23f18e03dc49 |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | c71d239df917, d30047bca3b5, d201d14804d3 |
| zune-inflate | 0.2.54 | MIT OR Apache-2.0 OR Zlib | gpui-752daf2fb234 |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | c71d239df917, d30047bca3b5, d201d14804d3 |

## Шрифт


Inter Variable, SIL Open Font License 1.1. Источник: https://github.com/rsms/inter. Полный текст: assets/fonts/Inter-OFL.txt и THIRD_PARTY_LICENSES.txt.

## Каталог игр

Каталог `assets/games.json` содержит 24017 записей, подготовленных из метаданных [TitleDB](https://github.com/blawar/titledb), revision `d424690446eec6f4484209b9a1c067c7f7511398`. Лицензия MIT, Copyright (c) 2019 Blake Warner. Источники и SHA-256 снимков перечислены в `assets/games-sources.json`. Полный текст лицензии включен в THIRD_PARTY_LICENSES.txt.


## Эквалайзер

Реализация peaking biquad использует формулы [W3C Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/). Модели OCR больше не загружаются и не используются. Исторические тексты лицензий сохранены в THIRD_PARTY_LICENSES.txt.


## Spatial upscaling shaders

NVIDIA Image Scaling 35e13ba316c98eeecf16f37eae70ce88019911f6 (MIT) and GPUOpen FidelityFX-FSR a21ffb8f6c13233ba336352bdff293894c706575 (MIT). Original sources, coefficient tables and full licenses retained in src/app/vendor. WGSL ports retain vendor equations; see docs/superpowers/plans/upscaling-task1-report.md.


## Optional NVIDIA RTX Video runtime

RTX Video SDK 1.1.0 is proprietary and is not covered by this repository's MIT license. Optional Windows packages include the release `nvngx_vsr.dll` and `console_link_ngx.dll`, a C-export bridge linked from the SDK's official static dispatch library. The complete `NVIDIA_RTX_Video_SDK_License.pdf` accompanies those binaries. SDK headers, libraries, samples and development runtime are not vendored. Obtain the SDK from NVIDIA and review its distribution terms before distributing packages containing it.

This software contains source code provided by NVIDIA Corporation. Integration follows the SDK's documented DX12 API and sample resource/parameter conventions.

## GPUI frontend

GPUI 0.2.2 uses Apache-2.0. Text-field layout derives from its input example; original attribution remains in src/gpui/widgets.rs. License copy: licenses/GPUI-Apache-2.0.txt, also included in the distributed THIRD_PARTY_LICENSES.txt.

## Источники уведомлений GPUI-зависимостей

Новые записи используют LICENSE/COPYING/NOTICE/COPYRIGHT из опубликованных архивов. Для отсутствующих файлов по возможности прочитан официальный upstream по SHA из `.cargo_vcs_info.json`.

Следующие пакеты не включают отдельный файл лицензии в опубликованный архив; использованы стандартные условия выбранной SPDX-лицензии и атрибуция из их Cargo.toml, без выдуманных дат или владельцев copyright:

- `convert_case 0.4.0`: MIT.
- `dtor 0.0.6`: Apache-2.0.
- `dtor-proc-macro 0.0.5`: Apache-2.0.
- `leak 0.1.2`: Apache-2.0.
- `leaky-cow 0.1.1`: Apache-2.0.
- `mac 0.1.1`: Apache-2.0.
- `pathfinder_geometry 0.5.1`: Apache-2.0.
- `seahash 4.1.0`: MIT.
- `simd_helpers 0.1.0`: MIT.
- `svg_fmt 0.4.5`: Apache-2.0.
- `zune-inflate 0.2.54`: Apache-2.0.

GPUI TextField adaptation retains the original notice: Copyright Zed Industries, Inc. Licensed under Apache-2.0.
