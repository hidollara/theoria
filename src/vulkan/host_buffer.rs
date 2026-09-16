//! Host-visible Vulkan transfer storage.

use std::{error::Error, ptr};

use ash::vk;

use crate::vulkan::presenter::RenderDevice;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct HostBuffer {
    device: RenderDevice,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
}

impl HostBuffer {
    pub(super) fn new(
        device: RenderDevice,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let raw = device.raw();
        let buffer = unsafe {
            raw.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?
        };
        let requirements = unsafe { raw.get_buffer_memory_requirements(buffer) };
        let Some(memory_type) = device.find_memory_type(
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        ) else {
            unsafe { raw.destroy_buffer(buffer, None) };
            return Err("no host-visible coherent Vulkan memory type for transfer".into());
        };
        let memory = match unsafe {
            raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type),
                None,
            )
        } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { raw.destroy_buffer(buffer, None) };
                return Err(error.into());
            }
        };
        if let Err(error) = unsafe { raw.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                raw.free_memory(memory, None);
                raw.destroy_buffer(buffer, None);
            }
            return Err(error.into());
        }
        Ok(Self {
            device,
            buffer,
            memory,
            size,
        })
    }

    pub(super) const fn raw(&self) -> vk::Buffer {
        self.buffer
    }

    pub(super) fn write(&self, bytes: &[u8]) -> Result<()> {
        if bytes.len() as vk::DeviceSize > self.size {
            return Err("transfer data exceeds its host buffer".into());
        }
        let mapped = unsafe {
            self.device.raw().map_memory(
                self.memory,
                0,
                bytes.len() as vk::DeviceSize,
                vk::MemoryMapFlags::empty(),
            )?
        };
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.cast(), bytes.len());
            self.device.raw().unmap_memory(self.memory);
        }
        Ok(())
    }

    pub(super) fn read(&self) -> Result<Vec<u8>> {
        let size = usize::try_from(self.size).map_err(|_| "host buffer is too large to read")?;
        let mapped = unsafe {
            self.device
                .raw()
                .map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())?
        };
        let mut bytes = vec![0; size];
        unsafe {
            ptr::copy_nonoverlapping(mapped.cast(), bytes.as_mut_ptr(), size);
            self.device.raw().unmap_memory(self.memory);
        }
        Ok(bytes)
    }
}

impl Drop for HostBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.raw().destroy_buffer(self.buffer, None);
            self.device.raw().free_memory(self.memory, None);
        }
    }
}
