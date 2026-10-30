//! Reusable GPU storage for procedurally deformed and variable-sized meshes.
use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device, Queue};

/// Cumulative traffic and allocation counters for placed meshes.
#[derive(Default, Copy, Clone, Debug)]
pub struct MeshUploadStats {
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub buffer_allocations: u64,
}

pub(crate) struct MeshBuffers {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub count: u32,
    index_data: Vec<u32>,
}

fn capacity(required: u64, limit: u64) -> u64 {
    assert!(required <= limit, "mesh exceeds the device buffer size limit");
    required.max(256).next_power_of_two().min(limit)
}

impl MeshBuffers {
    pub fn upload(
        slot: &mut Option<Self>, device: &Device, queue: &Queue,
        vertices: &[u8], indices: &[u32], stats: &mut MeshUploadStats,
    ) -> u64 {
        if vertices.is_empty() || indices.is_empty() {
            *slot = None;
            return 0;
        }
        assert!(indices.len() <= u32::MAX as usize, "mesh index count exceeds u32");
        let vb = vertices.len() as u64;
        let ib = std::mem::size_of_val(indices) as u64;
        let limit = device.limits().max_buffer_size;
        let mut allocate = |label, bytes, usage| {
            stats.buffer_allocations += 1;
            device.create_buffer(&BufferDescriptor {
                label: Some(label), size: capacity(bytes, limit),
                usage: usage | BufferUsages::COPY_DST, mapped_at_creation: false,
            })
        };
        let mesh = slot.get_or_insert_with(|| Self {
            vertices: allocate("dynamic mesh vertices", vb, BufferUsages::VERTEX),
            indices: allocate("dynamic mesh indices", ib, BufferUsages::INDEX),
            count: 0, index_data: Vec::new(),
        });
        if mesh.vertices.size() < vb {
            mesh.vertices = allocate("dynamic mesh vertices", vb, BufferUsages::VERTEX);
        }
        if mesh.indices.size() < ib {
            mesh.indices = allocate("dynamic mesh indices", ib, BufferUsages::INDEX);
            mesh.index_data.clear();
        }
        queue.write_buffer(&mesh.vertices, 0, vertices);
        stats.vertex_bytes += vb;
        // Rigid skinning changes positions/normals, not topology. Avoid
        // resending the index buffer on every animation frame.
        if mesh.index_data != indices {
            queue.write_buffer(&mesh.indices, 0, bytemuck::cast_slice(indices));
            mesh.index_data.clear();
            mesh.index_data.extend_from_slice(indices);
            stats.index_bytes += ib;
        }
        mesh.count = indices.len() as u32;
        mesh.vertices.size() + mesh.indices.size()
    }
}
