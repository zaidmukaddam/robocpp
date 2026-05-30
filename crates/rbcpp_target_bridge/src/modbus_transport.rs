// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::io;

use modbus::{Client, Coil};
use rbcpp_target::{ModbusArea, ModbusImage, ModbusPoint, ModbusTransport};

#[derive(Debug)]
pub struct SimModbusTransport {
    image: RefCell<ModbusImage>,
}

impl SimModbusTransport {
    pub fn new() -> Self {
        Self {
            image: RefCell::new(ModbusImage::default()),
        }
    }
}

impl Default for SimModbusTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl ModbusTransport for SimModbusTransport {
    fn read_coil(&self, unit_id: u8, address: u16) -> io::Result<bool> {
        Ok(self
            .image
            .borrow()
            .read(ModbusPoint::new(unit_id, ModbusArea::Coil, address))
            .as_bool()
            .unwrap_or(false))
    }

    fn read_discrete_input(&self, unit_id: u8, address: u16) -> io::Result<bool> {
        Ok(self
            .image
            .borrow()
            .read(ModbusPoint::new(
                unit_id,
                ModbusArea::DiscreteInput,
                address,
            ))
            .as_bool()
            .unwrap_or(false))
    }

    fn read_holding_register(&self, unit_id: u8, address: u16) -> io::Result<u16> {
        Ok(self
            .image
            .borrow()
            .read(ModbusPoint::new(
                unit_id,
                ModbusArea::HoldingRegister,
                address,
            ))
            .as_i64()
            .unwrap_or(0)
            .clamp(0, u16::MAX as i64) as u16)
    }

    fn read_input_register(&self, unit_id: u8, address: u16) -> io::Result<u16> {
        Ok(self
            .image
            .borrow()
            .read(ModbusPoint::new(
                unit_id,
                ModbusArea::InputRegister,
                address,
            ))
            .as_i64()
            .unwrap_or(0)
            .clamp(0, u16::MAX as i64) as u16)
    }

    fn write_coil(&mut self, unit_id: u8, address: u16, value: bool) -> io::Result<bool> {
        self.image.borrow_mut().write(
            ModbusPoint::new(unit_id, ModbusArea::Coil, address),
            &iec_ir::Value::Bool(value),
        );
        Ok(true)
    }

    fn write_holding_register(
        &mut self,
        unit_id: u8,
        address: u16,
        value: u16,
    ) -> io::Result<bool> {
        self.image.borrow_mut().write(
            ModbusPoint::new(unit_id, ModbusArea::HoldingRegister, address),
            &iec_ir::Value::Int(i64::from(value)),
        );
        Ok(true)
    }
}

#[derive(Debug)]
pub struct TcpModbusTransport {
    endpoint: String,
}

impl TcpModbusTransport {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    fn with_client<T>(
        &self,
        unit_id: u8,
        action: impl FnOnce(&mut modbus::tcp::Transport) -> modbus::Result<T>,
    ) -> io::Result<T> {
        let mut transport = modbus::tcp::Transport::new(&self.endpoint)
            .map_err(|error| io::Error::other(error.to_string()))?;
        transport.set_uid(unit_id);
        action(&mut transport).map_err(|error| io::Error::other(error.to_string()))
    }
}

impl ModbusTransport for TcpModbusTransport {
    fn read_coil(&self, unit_id: u8, address: u16) -> io::Result<bool> {
        self.with_client(unit_id, |client| {
            let coils = client.read_coils(address, 1)?;
            Ok(coils.first() == Some(&Coil::On))
        })
    }

    fn read_discrete_input(&self, unit_id: u8, address: u16) -> io::Result<bool> {
        self.with_client(unit_id, |client| {
            let inputs = client.read_discrete_inputs(address, 1)?;
            Ok(inputs.first() == Some(&Coil::On))
        })
    }

    fn read_holding_register(&self, unit_id: u8, address: u16) -> io::Result<u16> {
        self.with_client(unit_id, |client| {
            let registers = client.read_holding_registers(address, 1)?;
            Ok(registers.first().copied().unwrap_or(0))
        })
    }

    fn read_input_register(&self, unit_id: u8, address: u16) -> io::Result<u16> {
        self.with_client(unit_id, |client| {
            let registers = client.read_input_registers(address, 1)?;
            Ok(registers.first().copied().unwrap_or(0))
        })
    }

    fn write_coil(&mut self, unit_id: u8, address: u16, value: bool) -> io::Result<bool> {
        self.with_client(unit_id, |client| {
            client.write_single_coil(address, if value { Coil::On } else { Coil::Off })?;
            Ok(true)
        })
    }

    fn write_holding_register(
        &mut self,
        unit_id: u8,
        address: u16,
        value: u16,
    ) -> io::Result<bool> {
        self.with_client(unit_id, |client| {
            client.write_single_register(address, value)?;
            Ok(true)
        })
    }
}
