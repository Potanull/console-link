//! Anime4K 3D US: three learned convolutions, then a 2x pixel shuffle.
//! All inference stays on the presentation GPU; feature maps retain signed values.

struct Feature {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct Intermediates {
    size: [u32; 2],
    features: [Feature; 2],
    bindings: [wgpu::BindGroup; 4],
}

pub(super) struct Resources {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipelines: [wgpu::RenderPipeline; 4],
    intermediates: Option<Intermediates>,
}

impl Resources {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Anime4K CNN bindings"),
            entries: &[
                texture(0),
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Anime4K CNN layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Anime4K 3D US pretrained CNN"),
            source: wgpu::ShaderSource::Wgsl(include_str!("neural.wgsl").into()),
        });
        let pipelines = ["conv0", "conv1", "conv2", "reconstruct"].map(|entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: if entry == "reconstruct" {
                            wgpu::TextureFormat::Rgba8Unorm
                        } else {
                            wgpu::TextureFormat::Rgba16Float
                        },
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Anime4K base sampling"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            sampler,
            pipelines,
            intermediates: None,
        }
    }

    /// Called only when the source size changes. The caller also recreates its
    /// graded source at that point, so cached bindings never refer to old frames.
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        size: [u32; 2],
    ) -> Result<(), String> {
        if self.intermediates.as_ref().is_some_and(|i| i.size == size) {
            return Ok(());
        }
        // Drop old textures before allocating another size, keeping memory bounded.
        self.intermediates = None;
        let intermediates = super::upscale::checked_gpu_creation(device, || {
            let features = std::array::from_fn(|_| {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Anime4K signed feature map"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                Feature {
                    view: texture.create_view(&Default::default()),
                    _texture: texture,
                }
            });
            let bindings = [
                source,
                &features[0].view,
                &features[1].view,
                &features[0].view,
            ]
            .map(|input| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Anime4K layer inputs"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(input),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            });
            Intermediates {
                size,
                features,
                bindings,
            }
        })?;
        self.intermediates = Some(intermediates);
        Ok(())
    }

    pub(super) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        doubled_output: &wgpu::TextureView,
    ) {
        let i = self
            .intermediates
            .as_ref()
            .expect("CNN prepared before encoding");
        for (layer, output) in [
            &i.features[0].view,
            &i.features[1].view,
            &i.features[0].view,
            doubled_output,
        ]
        .into_iter()
        .enumerate()
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Anime4K CNN layer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipelines[layer]);
            pass.set_bind_group(0, &i.bindings[layer], &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
