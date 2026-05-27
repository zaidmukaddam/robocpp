// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{error, fmt};

use iec_ir::{AccessDirection, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoDirection {
    Input,
    Output,
    Memory,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoSymbol {
    pub name: String,
    pub location: String,
    pub direction: IoDirection,
    pub type_name: String,
}

impl IoSymbol {
    pub fn new(
        name: impl Into<String>,
        location: impl Into<String>,
        direction: IoDirection,
        type_name: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            location: location.into(),
            direction,
            type_name: type_name.into(),
        }
    }

    pub fn location_key(&self) -> String {
        canonical_key(&self.location)
    }

    pub fn name_key(&self) -> String {
        canonical_key(&self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoEncoding {
    Bool01,
    Decimal,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoBinding {
    pub path: PathBuf,
    pub encoding: IoEncoding,
}

#[derive(Debug, Clone, Default)]
pub struct FileBackedHal {
    bindings: BTreeMap<String, IoBinding>,
}

impl FileBackedHal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_location(
        mut self,
        location: impl AsRef<str>,
        path: impl Into<PathBuf>,
        encoding: IoEncoding,
    ) -> Self {
        self.bindings.insert(
            canonical_key(location.as_ref()),
            IoBinding {
                path: path.into(),
                encoding,
            },
        );
        self
    }

    pub fn bind_name(
        mut self,
        name: impl AsRef<str>,
        path: impl Into<PathBuf>,
        encoding: IoEncoding,
    ) -> Self {
        self.bindings.insert(
            canonical_key(name.as_ref()),
            IoBinding {
                path: path.into(),
                encoding,
            },
        );
        self
    }

    pub fn read(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        let Some(binding) = self.binding_for(symbol) else {
            return Ok(None);
        };
        let raw = fs::read_to_string(&binding.path)?;
        Ok(Some(decode_value(
            binding.encoding,
            raw.trim_end_matches(['\n', '\r']),
        )))
    }

    pub fn write(&self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        let Some(binding) = self.binding_for(symbol) else {
            return Ok(false);
        };
        if let Some(parent) = binding.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&binding.path, encode_value(binding.encoding, value))?;
        Ok(true)
    }

    fn binding_for(&self, symbol: &IoSymbol) -> Option<&IoBinding> {
        self.bindings
            .get(&symbol.location_key())
            .or_else(|| self.bindings.get(&symbol.name_key()))
    }
}

pub trait TargetHal {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>>;
    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool>;
}

impl TargetHal for FileBackedHal {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        self.read(symbol)
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        self.write(symbol, value)
    }
}

#[derive(Debug, Clone)]
pub struct RetainStore {
    root: PathBuf,
}

impl RetainStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load(&self, name: &str) -> io::Result<Option<Value>> {
        let path = self.path_for(name);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(path)?;
        Ok(Some(decode_typed_value(raw.trim_end_matches(['\n', '\r']))))
    }

    pub fn save(&self, name: &str, value: &Value) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::write(self.path_for(name), encode_typed_value(value))?;
        Ok(())
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.root
            .join(format!("{}.retain", safe_file_component(name)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TargetError {
    UnknownAccessPath(String),
    ReadOnlyAccessPath(String),
    MissingIoHal(String),
    Safety { name: String, reason: SafetyTrip },
    Io { name: String, message: String },
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAccessPath(name) => write!(f, "unknown VAR_ACCESS path '{name}'"),
            Self::ReadOnlyAccessPath(name) => write!(f, "VAR_ACCESS path '{name}' is READ_ONLY"),
            Self::MissingIoHal(name) => {
                write!(
                    f,
                    "VAR_ACCESS path '{name}' is bound to target I/O without a HAL"
                )
            }
            Self::Safety { name, reason } => {
                write!(
                    f,
                    "VAR_ACCESS path '{name}' safety gate blocked write: {reason}"
                )
            }
            Self::Io { name, message } => {
                write!(f, "VAR_ACCESS path '{name}' I/O error: {message}")
            }
        }
    }
}

impl error::Error for TargetError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessTarget {
    State(String),
    Io(IoSymbol),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPathBinding {
    pub name: String,
    pub target: String,
    pub direction: AccessDirection,
    pub type_name: String,
    pub target_binding: AccessTarget,
}

impl AccessPathBinding {
    pub fn state(
        name: impl Into<String>,
        target: impl Into<String>,
        direction: AccessDirection,
        type_name: impl Into<String>,
    ) -> Self {
        let target = target.into();
        Self {
            name: name.into(),
            target: target.clone(),
            direction,
            type_name: type_name.into(),
            target_binding: AccessTarget::State(target),
        }
    }

    pub fn io(name: impl Into<String>, symbol: IoSymbol, direction: AccessDirection) -> Self {
        let target = if symbol.location.is_empty() {
            symbol.name.clone()
        } else {
            symbol.location.clone()
        };
        Self {
            name: name.into(),
            target,
            direction,
            type_name: symbol.type_name.clone(),
            target_binding: AccessTarget::Io(symbol),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TargetState {
    values: BTreeMap<String, Value>,
    retained: BTreeSet<String>,
}

impl TargetState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, target: impl AsRef<str>, value: Value) {
        self.values.insert(canonical_key(target.as_ref()), value);
    }

    pub fn mark_retained(&mut self, target: impl AsRef<str>) {
        self.retained.insert(canonical_key(target.as_ref()));
    }

    pub fn read(&self, target: impl AsRef<str>) -> Option<Value> {
        self.values.get(&canonical_key(target.as_ref())).cloned()
    }

    pub fn write(&mut self, target: impl AsRef<str>, value: Value) {
        self.set(target, value);
    }

    pub fn load_retained(&mut self, store: &RetainStore) -> io::Result<()> {
        for target in self.retained.clone() {
            if let Some(value) = store.load(&target)? {
                self.values.insert(target, value);
            }
        }
        Ok(())
    }

    pub fn save_retained(&self, store: &RetainStore) -> io::Result<()> {
        for target in &self.retained {
            if let Some(value) = self.values.get(target) {
                store.save(target, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct AccessRuntime {
    bindings: BTreeMap<String, AccessPathBinding>,
    state: TargetState,
}

impl AccessRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> &TargetState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut TargetState {
        &mut self.state
    }

    pub fn register(&mut self, binding: AccessPathBinding) {
        self.bindings.insert(canonical_key(&binding.name), binding);
    }

    pub fn access_binding(&self, name: &str) -> Result<&AccessPathBinding, TargetError> {
        self.binding(name)
    }

    pub fn read_access(&self, name: &str) -> Result<Option<Value>, TargetError> {
        let binding = self.binding(name)?;
        match &binding.target_binding {
            AccessTarget::State(target) => Ok(self.state.read(target)),
            AccessTarget::Io(_) => Err(TargetError::MissingIoHal(binding.name.clone())),
        }
    }

    pub fn write_access(&mut self, name: &str, value: Value) -> Result<bool, TargetError> {
        let binding = self.binding(name)?.clone();
        self.ensure_writable(&binding)?;
        match binding.target_binding {
            AccessTarget::State(target) => {
                self.state.write(target, value);
                Ok(true)
            }
            AccessTarget::Io(_) => Err(TargetError::MissingIoHal(binding.name)),
        }
    }

    pub fn read_access_with_hal<H: TargetHal>(
        &self,
        name: &str,
        hal: &H,
    ) -> Result<Option<Value>, TargetError> {
        let binding = self.binding(name)?;
        match &binding.target_binding {
            AccessTarget::State(target) => Ok(self.state.read(target)),
            AccessTarget::Io(symbol) => hal.read_symbol(symbol).map_err(|err| TargetError::Io {
                name: binding.name.clone(),
                message: err.to_string(),
            }),
        }
    }

    pub fn write_access_with_hal<H: TargetHal>(
        &mut self,
        name: &str,
        value: Value,
        hal: &mut H,
    ) -> Result<bool, TargetError> {
        let binding = self.binding(name)?.clone();
        self.ensure_writable(&binding)?;
        match binding.target_binding {
            AccessTarget::State(target) => {
                self.state.write(target, value);
                Ok(true)
            }
            AccessTarget::Io(symbol) => {
                hal.write_symbol(&symbol, &value)
                    .map_err(|err| TargetError::Io {
                        name: binding.name,
                        message: err.to_string(),
                    })
            }
        }
    }

    pub fn load_retained(&mut self, store: &RetainStore) -> io::Result<()> {
        self.state.load_retained(store)
    }

    pub fn save_retained(&self, store: &RetainStore) -> io::Result<()> {
        self.state.save_retained(store)
    }

    fn binding(&self, name: &str) -> Result<&AccessPathBinding, TargetError> {
        self.bindings
            .get(&canonical_key(name))
            .ok_or_else(|| TargetError::UnknownAccessPath(name.to_string()))
    }

    fn ensure_writable(&self, binding: &AccessPathBinding) -> Result<(), TargetError> {
        if binding.direction == AccessDirection::ReadWrite {
            Ok(())
        } else {
            Err(TargetError::ReadOnlyAccessPath(binding.name.clone()))
        }
    }
}

#[derive(Debug, Clone)]
pub struct CycleWatchdog {
    timeout: Duration,
    last_pet: Instant,
}

impl CycleWatchdog {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            last_pet: Instant::now(),
        }
    }

    pub fn pet(&mut self) {
        self.last_pet = Instant::now();
    }

    pub fn expired(&self) -> bool {
        self.last_pet.elapsed() > self.timeout
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SafetyInputs {
    pub emergency_stop: bool,
    pub protective_stop: bool,
    pub operator_enable: bool,
    pub reset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafetyState {
    pub emergency_stop: bool,
    pub protective_stop: bool,
    pub watchdog_expired: bool,
    pub fault_latched: bool,
    pub outputs_enabled: bool,
}

impl Default for SafetyState {
    fn default() -> Self {
        Self {
            emergency_stop: false,
            protective_stop: false,
            watchdog_expired: false,
            fault_latched: false,
            outputs_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyTrip {
    EmergencyStop,
    ProtectiveStop,
    WatchdogExpired,
    FaultLatched,
    NotEnabled,
}

impl fmt::Display for SafetyTrip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmergencyStop => f.write_str("emergency stop is active"),
            Self::ProtectiveStop => f.write_str("protective stop is active"),
            Self::WatchdogExpired => f.write_str("cycle watchdog expired"),
            Self::FaultLatched => f.write_str("safety fault is latched"),
            Self::NotEnabled => f.write_str("operator enable is not active"),
        }
    }
}

impl error::Error for SafetyTrip {}

#[derive(Debug, Clone)]
pub struct SafetyGate {
    require_operator_enable: bool,
    state: SafetyState,
}

impl SafetyGate {
    pub fn new(require_operator_enable: bool) -> Self {
        Self {
            require_operator_enable,
            state: SafetyState::default(),
        }
    }

    pub fn state(&self) -> SafetyState {
        self.state
    }

    pub fn update(&mut self, inputs: SafetyInputs, watchdog_expired: bool) -> SafetyState {
        self.state.emergency_stop = inputs.emergency_stop;
        self.state.protective_stop = inputs.protective_stop;
        self.state.watchdog_expired = watchdog_expired;

        let active_trip = inputs.emergency_stop || inputs.protective_stop || watchdog_expired;
        if active_trip {
            self.state.fault_latched = true;
        } else if inputs.reset {
            self.state.fault_latched = false;
        }

        self.state.outputs_enabled =
            !self.state.fault_latched && (!self.require_operator_enable || inputs.operator_enable);
        self.state
    }

    pub fn check_output_write(&self, symbol: &IoSymbol) -> Result<(), SafetyTrip> {
        if symbol.direction != IoDirection::Output {
            return Ok(());
        }
        if self.state.emergency_stop {
            return Err(SafetyTrip::EmergencyStop);
        }
        if self.state.protective_stop {
            return Err(SafetyTrip::ProtectiveStop);
        }
        if self.state.watchdog_expired {
            return Err(SafetyTrip::WatchdogExpired);
        }
        if self.state.fault_latched {
            return Err(SafetyTrip::FaultLatched);
        }
        if !self.state.outputs_enabled {
            return Err(SafetyTrip::NotEnabled);
        }
        Ok(())
    }

    pub fn write_symbol<H: TargetHal>(
        &self,
        hal: &mut H,
        symbol: &IoSymbol,
        value: &Value,
    ) -> Result<bool, SafetyTrip> {
        self.check_output_write(symbol)?;
        hal.write_symbol(symbol, value)
            .map_err(|_| SafetyTrip::FaultLatched)
    }
}

#[derive(Debug, Clone)]
pub struct SafetyHal<H> {
    hal: H,
    gate: SafetyGate,
}

impl<H> SafetyHal<H> {
    pub fn new(hal: H, gate: SafetyGate) -> Self {
        Self { hal, gate }
    }

    pub fn gate(&self) -> &SafetyGate {
        &self.gate
    }

    pub fn gate_mut(&mut self) -> &mut SafetyGate {
        &mut self.gate
    }

    pub fn into_inner(self) -> H {
        self.hal
    }
}

impl<H: TargetHal> TargetHal for SafetyHal<H> {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        self.hal.read_symbol(symbol)
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        self.gate
            .check_output_write(symbol)
            .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))?;
        self.hal.write_symbol(symbol, value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetCycleReport {
    pub cycle: u64,
    pub elapsed: Duration,
    pub watchdog_expired: bool,
    pub retained_saved: bool,
    pub safety_state: Option<SafetyState>,
}

#[derive(Debug, Clone)]
pub struct TargetSupervisor<H> {
    access: AccessRuntime,
    hal: H,
    retain_store: Option<RetainStore>,
    watchdog: Option<CycleWatchdog>,
    safety_gate: Option<SafetyGate>,
    safety_inputs: SafetyInputs,
    cycle_started_at: Option<Instant>,
    cycle_count: u64,
    last_cycle_time: Option<Duration>,
}

impl<H> TargetSupervisor<H> {
    pub fn new(hal: H) -> Self {
        Self {
            access: AccessRuntime::new(),
            hal,
            retain_store: None,
            watchdog: None,
            safety_gate: None,
            safety_inputs: SafetyInputs::default(),
            cycle_started_at: None,
            cycle_count: 0,
            last_cycle_time: None,
        }
    }

    pub fn with_access_runtime(mut self, access: AccessRuntime) -> Self {
        self.access = access;
        self
    }

    pub fn with_retain_store(mut self, retain_store: RetainStore) -> Self {
        self.retain_store = Some(retain_store);
        self
    }

    pub fn with_watchdog(mut self, watchdog: CycleWatchdog) -> Self {
        self.watchdog = Some(watchdog);
        self
    }

    pub fn with_safety_gate(mut self, safety_gate: SafetyGate) -> Self {
        self.safety_gate = Some(safety_gate);
        self
    }

    pub fn access(&self) -> &AccessRuntime {
        &self.access
    }

    pub fn access_mut(&mut self) -> &mut AccessRuntime {
        &mut self.access
    }

    pub fn hal(&self) -> &H {
        &self.hal
    }

    pub fn hal_mut(&mut self) -> &mut H {
        &mut self.hal
    }

    pub fn into_inner(self) -> (AccessRuntime, H) {
        (self.access, self.hal)
    }

    pub fn cycle_count(&self) -> u64 {
        self.cycle_count
    }

    pub fn last_cycle_time(&self) -> Option<Duration> {
        self.last_cycle_time
    }

    pub fn set_safety_inputs(&mut self, inputs: SafetyInputs) {
        self.safety_inputs = inputs;
    }

    pub fn safety_state(&self) -> Option<SafetyState> {
        self.safety_gate.as_ref().map(SafetyGate::state)
    }

    pub fn restore_retained(&mut self) -> io::Result<()> {
        if let Some(store) = &self.retain_store {
            self.access.load_retained(store)?;
        }
        Ok(())
    }

    pub fn cold_start(&mut self) -> io::Result<()> {
        self.restore_retained()?;
        if let Some(watchdog) = &mut self.watchdog {
            watchdog.pet();
        }
        Ok(())
    }

    pub fn warm_start(&mut self) -> io::Result<()> {
        self.cold_start()
    }

    pub fn begin_cycle(&mut self) -> Option<SafetyState> {
        self.cycle_started_at = Some(Instant::now());
        self.refresh_safety_state()
    }

    pub fn end_cycle(&mut self) -> Result<TargetCycleReport, TargetError> {
        let cycle = self.cycle_count + 1;
        let elapsed = self
            .cycle_started_at
            .take()
            .map(|started| started.elapsed())
            .unwrap_or_default();
        self.last_cycle_time = Some(elapsed);

        let watchdog_expired = self.watchdog_expired();
        let safety_state = self.refresh_safety_state();
        let retained_saved = self.save_retained()?;

        if let Some(watchdog) = &mut self.watchdog {
            watchdog.pet();
        }
        self.cycle_count = cycle;

        Ok(TargetCycleReport {
            cycle,
            elapsed,
            watchdog_expired,
            retained_saved,
            safety_state,
        })
    }

    fn save_retained(&self) -> Result<bool, TargetError> {
        let Some(store) = &self.retain_store else {
            return Ok(false);
        };
        self.access
            .save_retained(store)
            .map_err(|err| TargetError::Io {
                name: "retain".to_string(),
                message: err.to_string(),
            })?;
        Ok(true)
    }

    fn refresh_safety_state(&mut self) -> Option<SafetyState> {
        let watchdog_expired = self.watchdog_expired();
        self.safety_gate
            .as_mut()
            .map(|gate| gate.update(self.safety_inputs, watchdog_expired))
    }

    fn watchdog_expired(&self) -> bool {
        self.watchdog.as_ref().is_some_and(CycleWatchdog::expired)
    }
}

impl<H: TargetHal> TargetSupervisor<H> {
    pub fn read_access(&self, name: &str) -> Result<Option<Value>, TargetError> {
        self.access.read_access_with_hal(name, &self.hal)
    }

    pub fn write_access(&mut self, name: &str, value: Value) -> Result<bool, TargetError> {
        let binding = self.access.access_binding(name)?.clone();
        if let (Some(gate), AccessTarget::Io(symbol)) = (&self.safety_gate, &binding.target_binding)
        {
            gate.check_output_write(symbol)
                .map_err(|reason| TargetError::Safety {
                    name: binding.name.clone(),
                    reason,
                })?;
        }
        self.access
            .write_access_with_hal(name, value, &mut self.hal)
    }
}

pub fn parse_mapping_line(root: &Path, line: &str) -> Option<(String, IoBinding)> {
    let mut parts = line.split(',').map(str::trim);
    let key = parts.next()?;
    let file = parts.next()?;
    let encoding = match parts
        .next()
        .unwrap_or("decimal")
        .to_ascii_lowercase()
        .as_str()
    {
        "bool" | "bool01" | "bit" => IoEncoding::Bool01,
        "text" | "string" => IoEncoding::Text,
        _ => IoEncoding::Decimal,
    };
    Some((
        canonical_key(key),
        IoBinding {
            path: root.join(file),
            encoding,
        },
    ))
}

pub fn load_mapping(root: impl AsRef<Path>, text: &str) -> FileBackedHal {
    let root = root.as_ref();
    let bindings = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| parse_mapping_line(root, line))
        .collect();
    FileBackedHal { bindings }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModbusArea {
    Coil,
    DiscreteInput,
    HoldingRegister,
    InputRegister,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModbusPoint {
    pub unit_id: u8,
    pub area: ModbusArea,
    pub address: u16,
}

impl ModbusPoint {
    pub fn new(unit_id: u8, area: ModbusArea, address: u16) -> Self {
        Self {
            unit_id,
            area,
            address,
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let mut parts = input.split(':').map(str::trim);
        let unit_id = parts.next()?.parse::<u8>().ok()?;
        let area = ModbusArea::parse(parts.next()?)?;
        let address = parts.next()?.parse::<u16>().ok()?;
        parts.next().is_none().then_some(Self {
            unit_id,
            area,
            address,
        })
    }
}

impl ModbusArea {
    pub fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "coil" | "coils" | "0x" | "0" => Some(Self::Coil),
            "discrete" | "discrete_input" | "discrete-input" | "1x" | "1" => {
                Some(Self::DiscreteInput)
            }
            "holding" | "holding_register" | "holding-register" | "4x" | "4" => {
                Some(Self::HoldingRegister)
            }
            "input" | "input_register" | "input-register" | "3x" | "3" => Some(Self::InputRegister),
            _ => None,
        }
    }

    pub fn writable(self) -> bool {
        matches!(self, Self::Coil | Self::HoldingRegister)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModbusImage {
    bits: BTreeMap<ModbusPoint, bool>,
    registers: BTreeMap<ModbusPoint, u16>,
}

impl ModbusImage {
    pub fn set_bit(&mut self, point: ModbusPoint, value: bool) {
        self.bits.insert(point, value);
    }

    pub fn set_register(&mut self, point: ModbusPoint, value: u16) {
        self.registers.insert(point, value);
    }

    pub fn read(&self, point: ModbusPoint) -> Value {
        match point.area {
            ModbusArea::Coil | ModbusArea::DiscreteInput => {
                Value::Bool(*self.bits.get(&point).unwrap_or(&false))
            }
            ModbusArea::HoldingRegister | ModbusArea::InputRegister => {
                Value::Int(i64::from(*self.registers.get(&point).unwrap_or(&0)))
            }
        }
    }

    pub fn write(&mut self, point: ModbusPoint, value: &Value) -> bool {
        if !point.area.writable() {
            return false;
        }
        match point.area {
            ModbusArea::Coil => {
                self.bits.insert(point, value.as_bool().unwrap_or(false));
                true
            }
            ModbusArea::HoldingRegister => {
                self.registers.insert(
                    point,
                    value.as_i64().unwrap_or(0).clamp(0, u16::MAX as i64) as u16,
                );
                true
            }
            ModbusArea::DiscreteInput | ModbusArea::InputRegister => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModbusHal {
    bindings: BTreeMap<String, ModbusPoint>,
    image: ModbusImage,
}

impl ModbusHal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_location(mut self, location: impl AsRef<str>, point: ModbusPoint) -> Self {
        self.bindings
            .insert(canonical_key(location.as_ref()), point);
        self
    }

    pub fn bind_name(mut self, name: impl AsRef<str>, point: ModbusPoint) -> Self {
        self.bindings.insert(canonical_key(name.as_ref()), point);
        self
    }

    pub fn image_mut(&mut self) -> &mut ModbusImage {
        &mut self.image
    }

    pub fn read(&self, symbol: &IoSymbol) -> Option<Value> {
        self.point_for(symbol).map(|point| self.image.read(point))
    }

    pub fn write(&mut self, symbol: &IoSymbol, value: &Value) -> bool {
        let Some(point) = self.point_for(symbol) else {
            return false;
        };
        self.image.write(point, value)
    }

    fn point_for(&self, symbol: &IoSymbol) -> Option<ModbusPoint> {
        self.bindings
            .get(&symbol.location_key())
            .or_else(|| self.bindings.get(&symbol.name_key()))
            .copied()
    }
}

impl TargetHal for ModbusHal {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        Ok(self.read(symbol))
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        Ok(self.write(symbol, value))
    }
}

pub fn load_modbus_mapping(text: &str) -> ModbusHal {
    let bindings = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, point) = line.split_once(',')?;
            Some((canonical_key(key), ModbusPoint::parse(point.trim())?))
        })
        .collect();
    ModbusHal {
        bindings,
        image: ModbusImage::default(),
    }
}

pub trait ModbusTransport {
    fn read_coil(&self, unit_id: u8, address: u16) -> io::Result<bool>;
    fn read_discrete_input(&self, unit_id: u8, address: u16) -> io::Result<bool>;
    fn read_holding_register(&self, unit_id: u8, address: u16) -> io::Result<u16>;
    fn read_input_register(&self, unit_id: u8, address: u16) -> io::Result<u16>;
    fn write_coil(&mut self, unit_id: u8, address: u16, value: bool) -> io::Result<bool>;
    fn write_holding_register(&mut self, unit_id: u8, address: u16, value: u16)
        -> io::Result<bool>;
}

impl ModbusTransport for ModbusImage {
    fn read_coil(&self, unit_id: u8, address: u16) -> io::Result<bool> {
        Ok(self
            .read(ModbusPoint::new(unit_id, ModbusArea::Coil, address))
            .as_bool()
            .unwrap_or(false))
    }

    fn read_discrete_input(&self, unit_id: u8, address: u16) -> io::Result<bool> {
        Ok(self
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
        Ok(self.write(
            ModbusPoint::new(unit_id, ModbusArea::Coil, address),
            &Value::Bool(value),
        ))
    }

    fn write_holding_register(
        &mut self,
        unit_id: u8,
        address: u16,
        value: u16,
    ) -> io::Result<bool> {
        Ok(self.write(
            ModbusPoint::new(unit_id, ModbusArea::HoldingRegister, address),
            &Value::Int(i64::from(value)),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ModbusTransportHal<T> {
    bindings: BTreeMap<String, ModbusPoint>,
    transport: T,
}

impl<T> ModbusTransportHal<T> {
    pub fn new(transport: T) -> Self {
        Self {
            bindings: BTreeMap::new(),
            transport,
        }
    }

    pub fn bind_location(mut self, location: impl AsRef<str>, point: ModbusPoint) -> Self {
        self.bindings
            .insert(canonical_key(location.as_ref()), point);
        self
    }

    pub fn bind_name(mut self, name: impl AsRef<str>, point: ModbusPoint) -> Self {
        self.bindings.insert(canonical_key(name.as_ref()), point);
        self
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn into_inner(self) -> T {
        self.transport
    }

    fn point_for(&self, symbol: &IoSymbol) -> Option<ModbusPoint> {
        self.bindings
            .get(&symbol.location_key())
            .or_else(|| self.bindings.get(&symbol.name_key()))
            .copied()
    }
}

impl<T: ModbusTransport> TargetHal for ModbusTransportHal<T> {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        let Some(point) = self.point_for(symbol) else {
            return Ok(None);
        };
        let value = match point.area {
            ModbusArea::Coil => {
                Value::Bool(self.transport.read_coil(point.unit_id, point.address)?)
            }
            ModbusArea::DiscreteInput => Value::Bool(
                self.transport
                    .read_discrete_input(point.unit_id, point.address)?,
            ),
            ModbusArea::HoldingRegister => Value::Int(i64::from(
                self.transport
                    .read_holding_register(point.unit_id, point.address)?,
            )),
            ModbusArea::InputRegister => Value::Int(i64::from(
                self.transport
                    .read_input_register(point.unit_id, point.address)?,
            )),
        };
        Ok(Some(value))
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        let Some(point) = self.point_for(symbol) else {
            return Ok(false);
        };
        match point.area {
            ModbusArea::Coil => self.transport.write_coil(
                point.unit_id,
                point.address,
                value.as_bool().unwrap_or(false),
            ),
            ModbusArea::HoldingRegister => self.transport.write_holding_register(
                point.unit_id,
                point.address,
                value.as_i64().unwrap_or(0).clamp(0, u16::MAX as i64) as u16,
            ),
            ModbusArea::DiscreteInput | ModbusArea::InputRegister => Ok(false),
        }
    }
}

pub fn load_modbus_transport_mapping<T>(transport: T, text: &str) -> ModbusTransportHal<T> {
    let bindings = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, point) = line.split_once(',')?;
            Some((canonical_key(key), ModbusPoint::parse(point.trim())?))
        })
        .collect();
    ModbusTransportHal {
        bindings,
        transport,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EtherCatArea {
    Input,
    Output,
}

impl EtherCatArea {
    pub fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "input" | "inputs" | "rx" | "pdo_in" | "pdo-in" => Some(Self::Input),
            "output" | "outputs" | "tx" | "pdo_out" | "pdo-out" => Some(Self::Output),
            _ => None,
        }
    }

    pub fn writable(self) -> bool {
        matches!(self, Self::Output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EtherCatValueType {
    Bool,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
}

impl EtherCatValueType {
    pub fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "bool" | "bit" => Some(Self::Bool),
            "u8" | "byte" | "usint" => Some(Self::U8),
            "i8" | "sint" => Some(Self::I8),
            "u16" | "word" | "uint" => Some(Self::U16),
            "i16" | "int" => Some(Self::I16),
            "u32" | "dword" | "udint" => Some(Self::U32),
            "i32" | "dint" => Some(Self::I32),
            "u64" | "lword" | "ulint" => Some(Self::U64),
            "i64" | "lint" => Some(Self::I64),
            _ => None,
        }
    }

    fn byte_len(self) -> usize {
        match self {
            Self::Bool | Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 => 4,
            Self::U64 | Self::I64 => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EtherCatPdoPoint {
    pub slave: u16,
    pub index: u16,
    pub subindex: u8,
    pub area: EtherCatArea,
    pub byte_offset: usize,
    pub bit_offset: u8,
    pub value_type: EtherCatValueType,
}

impl EtherCatPdoPoint {
    pub fn new(
        slave: u16,
        index: u16,
        subindex: u8,
        area: EtherCatArea,
        byte_offset: usize,
        bit_offset: u8,
        value_type: EtherCatValueType,
    ) -> Self {
        Self {
            slave,
            index,
            subindex,
            area,
            byte_offset,
            bit_offset,
            value_type,
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let mut parts = input.split(':').map(str::trim);
        let slave = parse_u16(parts.next()?)?;
        let index = parse_u16(parts.next()?)?;
        let subindex = parse_u8(parts.next()?)?;
        let area = EtherCatArea::parse(parts.next()?)?;
        let (byte_offset, bit_offset) = parse_byte_bit(parts.next()?)?;
        let value_type = EtherCatValueType::parse(parts.next()?)?;
        parts.next().is_none().then_some(Self {
            slave,
            index,
            subindex,
            area,
            byte_offset,
            bit_offset,
            value_type,
        })
    }
}

#[derive(Debug, Clone)]
pub struct EtherCatPdoImage {
    inputs: Vec<u8>,
    outputs: Vec<u8>,
}

impl EtherCatPdoImage {
    pub fn new(input_bytes: usize, output_bytes: usize) -> Self {
        Self {
            inputs: vec![0; input_bytes],
            outputs: vec![0; output_bytes],
        }
    }

    pub fn input_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.inputs
    }

    pub fn output_bytes(&self) -> &[u8] {
        &self.outputs
    }

    pub fn read(&self, point: EtherCatPdoPoint) -> Option<Value> {
        let bytes = self.area_bytes(point.area);
        read_pdo_value(bytes, point)
    }

    pub fn write_input_for_simulation(&mut self, point: EtherCatPdoPoint, value: &Value) -> bool {
        let bytes = &mut self.inputs;
        write_pdo_value(bytes, point, value)
    }

    pub fn write(&mut self, point: EtherCatPdoPoint, value: &Value) -> bool {
        if !point.area.writable() {
            return false;
        }
        let bytes = &mut self.outputs;
        write_pdo_value(bytes, point, value)
    }

    fn area_bytes(&self, area: EtherCatArea) -> &[u8] {
        match area {
            EtherCatArea::Input => &self.inputs,
            EtherCatArea::Output => &self.outputs,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EtherCatHal {
    bindings: BTreeMap<String, EtherCatPdoPoint>,
    image: EtherCatPdoImage,
}

impl EtherCatHal {
    pub fn new(input_bytes: usize, output_bytes: usize) -> Self {
        Self {
            bindings: BTreeMap::new(),
            image: EtherCatPdoImage::new(input_bytes, output_bytes),
        }
    }

    pub fn bind_location(mut self, location: impl AsRef<str>, point: EtherCatPdoPoint) -> Self {
        self.bindings
            .insert(canonical_key(location.as_ref()), point);
        self
    }

    pub fn bind_name(mut self, name: impl AsRef<str>, point: EtherCatPdoPoint) -> Self {
        self.bindings.insert(canonical_key(name.as_ref()), point);
        self
    }

    pub fn image_mut(&mut self) -> &mut EtherCatPdoImage {
        &mut self.image
    }

    pub fn image(&self) -> &EtherCatPdoImage {
        &self.image
    }

    pub fn read(&self, symbol: &IoSymbol) -> Option<Value> {
        self.point_for(symbol)
            .and_then(|point| self.image.read(point))
    }

    pub fn write(&mut self, symbol: &IoSymbol, value: &Value) -> bool {
        let Some(point) = self.point_for(symbol) else {
            return false;
        };
        self.image.write(point, value)
    }

    fn point_for(&self, symbol: &IoSymbol) -> Option<EtherCatPdoPoint> {
        self.bindings
            .get(&symbol.location_key())
            .or_else(|| self.bindings.get(&symbol.name_key()))
            .copied()
    }
}

impl TargetHal for EtherCatHal {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        Ok(self.read(symbol))
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        Ok(self.write(symbol, value))
    }
}

pub fn load_ethercat_mapping(input_bytes: usize, output_bytes: usize, text: &str) -> EtherCatHal {
    let bindings = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, point) = line.split_once(',')?;
            Some((canonical_key(key), EtherCatPdoPoint::parse(point.trim())?))
        })
        .collect();
    EtherCatHal {
        bindings,
        image: EtherCatPdoImage::new(input_bytes, output_bytes),
    }
}

pub trait EtherCatTransport {
    fn read_pdo(
        &self,
        area: EtherCatArea,
        byte_offset: usize,
        byte_len: usize,
    ) -> io::Result<Vec<u8>>;
    fn write_pdo(
        &mut self,
        area: EtherCatArea,
        byte_offset: usize,
        bytes: &[u8],
    ) -> io::Result<bool>;
}

impl EtherCatTransport for EtherCatPdoImage {
    fn read_pdo(
        &self,
        area: EtherCatArea,
        byte_offset: usize,
        byte_len: usize,
    ) -> io::Result<Vec<u8>> {
        let bytes = match area {
            EtherCatArea::Input => &self.inputs,
            EtherCatArea::Output => &self.outputs,
        };
        if byte_offset + byte_len > bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "EtherCAT PDO read is outside the process image",
            ));
        }
        Ok(bytes[byte_offset..byte_offset + byte_len].to_vec())
    }

    fn write_pdo(
        &mut self,
        area: EtherCatArea,
        byte_offset: usize,
        bytes: &[u8],
    ) -> io::Result<bool> {
        if !area.writable() {
            return Ok(false);
        }
        if byte_offset + bytes.len() > self.outputs.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "EtherCAT PDO write is outside the process image",
            ));
        }
        self.outputs[byte_offset..byte_offset + bytes.len()].copy_from_slice(bytes);
        Ok(true)
    }
}

#[derive(Debug, Clone)]
pub struct EtherCatTransportHal<T> {
    bindings: BTreeMap<String, EtherCatPdoPoint>,
    transport: T,
}

impl<T> EtherCatTransportHal<T> {
    pub fn new(transport: T) -> Self {
        Self {
            bindings: BTreeMap::new(),
            transport,
        }
    }

    pub fn bind_location(mut self, location: impl AsRef<str>, point: EtherCatPdoPoint) -> Self {
        self.bindings
            .insert(canonical_key(location.as_ref()), point);
        self
    }

    pub fn bind_name(mut self, name: impl AsRef<str>, point: EtherCatPdoPoint) -> Self {
        self.bindings.insert(canonical_key(name.as_ref()), point);
        self
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn into_inner(self) -> T {
        self.transport
    }

    fn point_for(&self, symbol: &IoSymbol) -> Option<EtherCatPdoPoint> {
        self.bindings
            .get(&symbol.location_key())
            .or_else(|| self.bindings.get(&symbol.name_key()))
            .copied()
    }
}

impl<T: EtherCatTransport> TargetHal for EtherCatTransportHal<T> {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        let Some(point) = self.point_for(symbol) else {
            return Ok(None);
        };
        let bytes =
            self.transport
                .read_pdo(point.area, point.byte_offset, point.value_type.byte_len())?;
        let local = EtherCatPdoPoint {
            byte_offset: 0,
            ..point
        };
        Ok(read_pdo_value(&bytes, local))
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        let Some(point) = self.point_for(symbol) else {
            return Ok(false);
        };
        if !point.area.writable() {
            return Ok(false);
        }
        let len = point.value_type.byte_len();
        let mut bytes = self
            .transport
            .read_pdo(point.area, point.byte_offset, len)?;
        let local = EtherCatPdoPoint {
            byte_offset: 0,
            ..point
        };
        if !write_pdo_value(&mut bytes, local, value) {
            return Ok(false);
        }
        self.transport
            .write_pdo(point.area, point.byte_offset, &bytes)
    }
}

pub fn load_ethercat_transport_mapping<T>(transport: T, text: &str) -> EtherCatTransportHal<T> {
    let bindings = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, point) = line.split_once(',')?;
            Some((canonical_key(key), EtherCatPdoPoint::parse(point.trim())?))
        })
        .collect();
    EtherCatTransportHal {
        bindings,
        transport,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ros2Direction {
    Subscribe,
    Publish,
    Parameter,
}

impl Ros2Direction {
    pub fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "sub" | "subscribe" | "subscription" | "input" => Some(Self::Subscribe),
            "pub" | "publish" | "publisher" | "output" => Some(Self::Publish),
            "param" | "parameter" | "cfg" | "config" => Some(Self::Parameter),
            _ => None,
        }
    }

    pub fn readable(self) -> bool {
        matches!(self, Self::Subscribe | Self::Parameter)
    }

    pub fn writable(self) -> bool {
        matches!(self, Self::Publish | Self::Parameter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ros2Point {
    pub direction: Ros2Direction,
    pub name: String,
}

impl Ros2Point {
    pub fn new(direction: Ros2Direction, name: impl Into<String>) -> Self {
        Self {
            direction,
            name: name.into(),
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let (direction, name) = input.split_once(':')?;
        let direction = Ros2Direction::parse(direction.trim())?;
        let name = name.trim();
        (!name.is_empty()).then(|| Self::new(direction, name))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ros2Message {
    pub topic: String,
    pub value: Value,
}

#[derive(Debug, Clone, Default)]
pub struct Ros2Bridge {
    bindings: BTreeMap<String, Ros2Point>,
    values: BTreeMap<String, Value>,
    published: Vec<Ros2Message>,
}

impl Ros2Bridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_location(mut self, location: impl AsRef<str>, point: Ros2Point) -> Self {
        self.bindings
            .insert(canonical_key(location.as_ref()), point);
        self
    }

    pub fn bind_name(mut self, name: impl AsRef<str>, point: Ros2Point) -> Self {
        self.bindings.insert(canonical_key(name.as_ref()), point);
        self
    }

    pub fn receive(&mut self, topic: impl AsRef<str>, value: Value) {
        self.values.insert(topic_key(topic.as_ref()), value);
    }

    pub fn set_parameter(&mut self, name: impl AsRef<str>, value: Value) {
        self.values.insert(topic_key(name.as_ref()), value);
    }

    pub fn published(&self) -> &[Ros2Message] {
        &self.published
    }

    pub fn take_published(&mut self) -> Vec<Ros2Message> {
        std::mem::take(&mut self.published)
    }

    pub fn read(&self, symbol: &IoSymbol) -> Option<Value> {
        let point = self.point_for(symbol)?;
        if !point.direction.readable() {
            return None;
        }
        self.values.get(&topic_key(&point.name)).cloned()
    }

    pub fn write(&mut self, symbol: &IoSymbol, value: &Value) -> bool {
        let Some(point) = self.point_for(symbol).cloned() else {
            return false;
        };
        if !point.direction.writable() {
            return false;
        }
        if point.direction == Ros2Direction::Parameter {
            self.values.insert(topic_key(&point.name), value.clone());
        } else {
            self.published.push(Ros2Message {
                topic: point.name,
                value: value.clone(),
            });
        }
        true
    }

    fn point_for(&self, symbol: &IoSymbol) -> Option<&Ros2Point> {
        self.bindings
            .get(&symbol.location_key())
            .or_else(|| self.bindings.get(&symbol.name_key()))
    }
}

impl TargetHal for Ros2Bridge {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        Ok(self.read(symbol))
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        Ok(self.write(symbol, value))
    }
}

pub fn load_ros2_mapping(text: &str) -> Ros2Bridge {
    let bindings = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, point) = line.split_once(',')?;
            Some((canonical_key(key), Ros2Point::parse(point.trim())?))
        })
        .collect();
    Ros2Bridge {
        bindings,
        values: BTreeMap::new(),
        published: Vec::new(),
    }
}

pub trait Ros2Transport {
    fn read_subscription(&self, topic: &str) -> io::Result<Option<Value>>;
    fn read_parameter(&self, name: &str) -> io::Result<Option<Value>>;
    fn publish(&mut self, topic: &str, value: &Value) -> io::Result<bool>;
    fn set_parameter(&mut self, name: &str, value: &Value) -> io::Result<bool>;
}

impl Ros2Transport for Ros2Bridge {
    fn read_subscription(&self, topic: &str) -> io::Result<Option<Value>> {
        Ok(self.values.get(&topic_key(topic)).cloned())
    }

    fn read_parameter(&self, name: &str) -> io::Result<Option<Value>> {
        Ok(self.values.get(&topic_key(name)).cloned())
    }

    fn publish(&mut self, topic: &str, value: &Value) -> io::Result<bool> {
        self.published.push(Ros2Message {
            topic: topic.to_string(),
            value: value.clone(),
        });
        Ok(true)
    }

    fn set_parameter(&mut self, name: &str, value: &Value) -> io::Result<bool> {
        self.values.insert(topic_key(name), value.clone());
        Ok(true)
    }
}

#[derive(Debug, Clone)]
pub struct Ros2TransportHal<T> {
    bindings: BTreeMap<String, Ros2Point>,
    transport: T,
}

impl<T> Ros2TransportHal<T> {
    pub fn new(transport: T) -> Self {
        Self {
            bindings: BTreeMap::new(),
            transport,
        }
    }

    pub fn bind_location(mut self, location: impl AsRef<str>, point: Ros2Point) -> Self {
        self.bindings
            .insert(canonical_key(location.as_ref()), point);
        self
    }

    pub fn bind_name(mut self, name: impl AsRef<str>, point: Ros2Point) -> Self {
        self.bindings.insert(canonical_key(name.as_ref()), point);
        self
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn into_inner(self) -> T {
        self.transport
    }

    fn point_for(&self, symbol: &IoSymbol) -> Option<&Ros2Point> {
        self.bindings
            .get(&symbol.location_key())
            .or_else(|| self.bindings.get(&symbol.name_key()))
    }
}

impl<T: Ros2Transport> TargetHal for Ros2TransportHal<T> {
    fn read_symbol(&self, symbol: &IoSymbol) -> io::Result<Option<Value>> {
        let Some(point) = self.point_for(symbol) else {
            return Ok(None);
        };
        match point.direction {
            Ros2Direction::Subscribe => self.transport.read_subscription(&point.name),
            Ros2Direction::Parameter => self.transport.read_parameter(&point.name),
            Ros2Direction::Publish => Ok(None),
        }
    }

    fn write_symbol(&mut self, symbol: &IoSymbol, value: &Value) -> io::Result<bool> {
        let Some(point) = self.point_for(symbol).cloned() else {
            return Ok(false);
        };
        match point.direction {
            Ros2Direction::Publish => self.transport.publish(&point.name, value),
            Ros2Direction::Parameter => self.transport.set_parameter(&point.name, value),
            Ros2Direction::Subscribe => Ok(false),
        }
    }
}

pub fn load_ros2_transport_mapping<T>(transport: T, text: &str) -> Ros2TransportHal<T> {
    let bindings = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, point) = line.split_once(',')?;
            Some((canonical_key(key), Ros2Point::parse(point.trim())?))
        })
        .collect();
    Ros2TransportHal {
        bindings,
        transport,
    }
}

fn decode_value(encoding: IoEncoding, raw: &str) -> Value {
    match encoding {
        IoEncoding::Bool01 => Value::Bool(matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "high"
        )),
        IoEncoding::Decimal => raw
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or(Value::Int(0)),
        IoEncoding::Text => Value::String(raw.to_string()),
    }
}

fn encode_value(encoding: IoEncoding, value: &Value) -> String {
    match encoding {
        IoEncoding::Bool01 => {
            if value.as_bool().unwrap_or(false) {
                "1\n".to_string()
            } else {
                "0\n".to_string()
            }
        }
        IoEncoding::Decimal => format!("{}\n", value.as_i64().unwrap_or(0)),
        IoEncoding::Text => match value {
            Value::String(value) | Value::WString(value) => format!("{value}\n"),
            _ => format!("{value}\n"),
        },
    }
}

fn decode_typed_value(raw: &str) -> Value {
    let Some((kind, value)) = raw.split_once(':') else {
        return Value::String(raw.to_string());
    };
    match kind {
        "BOOL" => Value::Bool(matches!(value, "1" | "TRUE" | "true")),
        "INT" => value
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or(Value::Int(0)),
        "REAL" => value
            .parse::<f64>()
            .map(Value::Real)
            .unwrap_or(Value::Real(0.0)),
        "WSTRING" => Value::WString(value.to_string()),
        "TIME" => value
            .parse::<i128>()
            .map(Value::TimeMs)
            .unwrap_or(Value::TimeMs(0)),
        "STRING" => Value::String(value.to_string()),
        _ => Value::String(value.to_string()),
    }
}

fn encode_typed_value(value: &Value) -> String {
    match value {
        Value::Bool(value) => format!("BOOL:{}\n", if *value { 1 } else { 0 }),
        Value::Int(value) => format!("INT:{value}\n"),
        Value::Real(value) => format!("REAL:{value}\n"),
        Value::String(value) => format!("STRING:{value}\n"),
        Value::WString(value) => format!("WSTRING:{value}\n"),
        Value::TimeMs(value) => format!("TIME:{value}\n"),
        Value::Array(_) | Value::Struct(_) | Value::Unit => format!("STRING:{value}\n"),
    }
}

fn canonical_key(input: &str) -> String {
    input.trim().to_ascii_uppercase()
}

fn topic_key(input: &str) -> String {
    input.trim().to_string()
}

fn safe_file_component(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

fn parse_u16(input: &str) -> Option<u16> {
    input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
        .map(|hex| u16::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| input.parse::<u16>().ok())
}

fn parse_u8(input: &str) -> Option<u8> {
    input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
        .map(|hex| u8::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| input.parse::<u8>().ok())
}

fn parse_byte_bit(input: &str) -> Option<(usize, u8)> {
    let (byte, bit) = input.split_once('.').unwrap_or((input, "0"));
    let byte_offset = byte.parse::<usize>().ok()?;
    let bit_offset = bit.parse::<u8>().ok()?;
    (bit_offset < 8).then_some((byte_offset, bit_offset))
}

fn read_pdo_value(bytes: &[u8], point: EtherCatPdoPoint) -> Option<Value> {
    if point.byte_offset + point.value_type.byte_len() > bytes.len() {
        return None;
    }
    match point.value_type {
        EtherCatValueType::Bool => Some(Value::Bool(read_bit(
            bytes,
            point.byte_offset,
            point.bit_offset,
        )?)),
        EtherCatValueType::U8 => Some(Value::Int(i64::from(bytes[point.byte_offset]))),
        EtherCatValueType::I8 => Some(Value::Int(i64::from(bytes[point.byte_offset] as i8))),
        EtherCatValueType::U16 => {
            let mut raw = [0_u8; 2];
            raw.copy_from_slice(&bytes[point.byte_offset..point.byte_offset + 2]);
            Some(Value::Int(i64::from(u16::from_le_bytes(raw))))
        }
        EtherCatValueType::I16 => {
            let mut raw = [0_u8; 2];
            raw.copy_from_slice(&bytes[point.byte_offset..point.byte_offset + 2]);
            Some(Value::Int(i64::from(i16::from_le_bytes(raw))))
        }
        EtherCatValueType::U32 => {
            let mut raw = [0_u8; 4];
            raw.copy_from_slice(&bytes[point.byte_offset..point.byte_offset + 4]);
            Some(Value::Int(i64::from(u32::from_le_bytes(raw))))
        }
        EtherCatValueType::I32 => {
            let mut raw = [0_u8; 4];
            raw.copy_from_slice(&bytes[point.byte_offset..point.byte_offset + 4]);
            Some(Value::Int(i64::from(i32::from_le_bytes(raw))))
        }
        EtherCatValueType::U64 => {
            let mut raw = [0_u8; 8];
            raw.copy_from_slice(&bytes[point.byte_offset..point.byte_offset + 8]);
            Some(Value::Int(
                u64::from_le_bytes(raw).min(i64::MAX as u64) as i64
            ))
        }
        EtherCatValueType::I64 => {
            let mut raw = [0_u8; 8];
            raw.copy_from_slice(&bytes[point.byte_offset..point.byte_offset + 8]);
            Some(Value::Int(i64::from_le_bytes(raw)))
        }
    }
}

fn write_pdo_value(bytes: &mut [u8], point: EtherCatPdoPoint, value: &Value) -> bool {
    if point.byte_offset + point.value_type.byte_len() > bytes.len() {
        return false;
    }
    match point.value_type {
        EtherCatValueType::Bool => write_bit(
            bytes,
            point.byte_offset,
            point.bit_offset,
            value.as_bool().unwrap_or(false),
        ),
        EtherCatValueType::U8 => {
            bytes[point.byte_offset] = value.as_i64().unwrap_or(0).clamp(0, u8::MAX as i64) as u8;
            true
        }
        EtherCatValueType::I8 => {
            bytes[point.byte_offset] = value
                .as_i64()
                .unwrap_or(0)
                .clamp(i8::MIN as i64, i8::MAX as i64) as i8
                as u8;
            true
        }
        EtherCatValueType::U16 => write_int_bytes(bytes, point, value, 0, u16::MAX as i64, 2),
        EtherCatValueType::I16 => {
            write_int_bytes(bytes, point, value, i16::MIN as i64, i16::MAX as i64, 2)
        }
        EtherCatValueType::U32 => write_int_bytes(bytes, point, value, 0, u32::MAX as i64, 4),
        EtherCatValueType::I32 => {
            write_int_bytes(bytes, point, value, i32::MIN as i64, i32::MAX as i64, 4)
        }
        EtherCatValueType::U64 => write_u64_bytes(bytes, point, value),
        EtherCatValueType::I64 => write_i64_bytes(bytes, point, value),
    }
}

fn read_bit(bytes: &[u8], byte_offset: usize, bit_offset: u8) -> Option<bool> {
    (bit_offset < 8 && byte_offset < bytes.len())
        .then(|| (bytes[byte_offset] & (1_u8 << bit_offset)) != 0)
}

fn write_bit(bytes: &mut [u8], byte_offset: usize, bit_offset: u8, value: bool) -> bool {
    if bit_offset >= 8 || byte_offset >= bytes.len() {
        return false;
    }
    if value {
        bytes[byte_offset] |= 1_u8 << bit_offset;
    } else {
        bytes[byte_offset] &= !(1_u8 << bit_offset);
    }
    true
}

fn write_int_bytes(
    bytes: &mut [u8],
    point: EtherCatPdoPoint,
    value: &Value,
    low: i64,
    high: i64,
    len: usize,
) -> bool {
    let value = value.as_i64().unwrap_or(0).clamp(low, high);
    let raw = value.to_le_bytes();
    bytes[point.byte_offset..point.byte_offset + len].copy_from_slice(&raw[..len]);
    true
}

fn write_u64_bytes(bytes: &mut [u8], point: EtherCatPdoPoint, value: &Value) -> bool {
    let value = value.as_i64().unwrap_or(0).max(0) as u64;
    bytes[point.byte_offset..point.byte_offset + 8].copy_from_slice(&value.to_le_bytes());
    true
}

fn write_i64_bytes(bytes: &mut [u8], point: EtherCatPdoPoint, value: &Value) -> bool {
    let value = value.as_i64().unwrap_or(0);
    bytes[point.byte_offset..point.byte_offset + 8].copy_from_slice(&value.to_le_bytes());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rbcpp_target_{name}_{}", std::process::id()))
    }

    #[test]
    fn file_backed_hal_reads_and_writes_locations() {
        let root = unique_temp("io");
        let input_path = root.join("gpio/in0/value");
        let output_path = root.join("gpio/out0/value");
        fs::create_dir_all(input_path.parent().unwrap()).unwrap();
        fs::write(&input_path, "1\n").unwrap();

        let hal = FileBackedHal::new()
            .bind_location("%IX0.0", &input_path, IoEncoding::Bool01)
            .bind_location("%QX0.0", &output_path, IoEncoding::Bool01);

        let input = IoSymbol::new("Start", "%IX0.0", IoDirection::Input, "BOOL");
        assert_eq!(hal.read(&input).unwrap(), Some(Value::Bool(true)));

        let output = IoSymbol::new("Motor", "%QX0.0", IoDirection::Output, "BOOL");
        assert!(hal.write(&output, &Value::Bool(true)).unwrap());
        assert_eq!(fs::read_to_string(output_path).unwrap(), "1\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mapping_loader_supports_named_and_located_entries() {
        let root = unique_temp("mapping");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("memory.txt"), "42\n").unwrap();
        fs::write(root.join("flag.txt"), "0\n").unwrap();

        let hal = load_mapping(
            &root,
            r#"
            # key,path,encoding
            %MW0,memory.txt,decimal
            StartFlag,flag.txt,bool
            "#,
        );

        let memory = IoSymbol::new("Memory", "%MW0", IoDirection::Memory, "INT");
        assert_eq!(hal.read(&memory).unwrap(), Some(Value::Int(42)));

        let named = IoSymbol::new("StartFlag", "", IoDirection::Input, "BOOL");
        assert_eq!(hal.read(&named).unwrap(), Some(Value::Bool(false)));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retain_store_round_trips_typed_values() {
        let root = unique_temp("retain");
        let store = RetainStore::new(&root);

        store.save("Counter.CV", &Value::Int(17)).unwrap();
        store.save("Armed", &Value::Bool(true)).unwrap();
        store
            .save("Label", &Value::String("robot".to_string()))
            .unwrap();

        assert_eq!(store.load("Counter.CV").unwrap(), Some(Value::Int(17)));
        assert_eq!(store.load("Armed").unwrap(), Some(Value::Bool(true)));
        assert_eq!(
            store.load("Label").unwrap(),
            Some(Value::String("robot".to_string()))
        );
        assert_eq!(store.load("Missing").unwrap(), None);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn access_runtime_enforces_direction_and_retained_state() {
        let root = unique_temp("access_retain");
        let store = RetainStore::new(&root);
        let mut runtime = AccessRuntime::new();
        runtime.register(AccessPathBinding::state(
            "PublicCount",
            "Config.Resource.Program.Count",
            AccessDirection::ReadWrite,
            "DINT",
        ));
        runtime.register(AccessPathBinding::state(
            "Status",
            "Config.Resource.Program.Status",
            AccessDirection::ReadOnly,
            "BOOL",
        ));
        runtime
            .state_mut()
            .set("Config.Resource.Program.Count", Value::Int(5));
        runtime
            .state_mut()
            .set("Config.Resource.Program.Status", Value::Bool(true));
        runtime
            .state_mut()
            .mark_retained("Config.Resource.Program.Count");

        assert_eq!(runtime.read_access("PublicCount"), Ok(Some(Value::Int(5))));
        assert_eq!(
            runtime.write_access("Status", Value::Bool(false)),
            Err(TargetError::ReadOnlyAccessPath("Status".to_string()))
        );
        assert_eq!(runtime.write_access("PublicCount", Value::Int(9)), Ok(true));
        runtime.save_retained(&store).unwrap();

        let mut restored = AccessRuntime::new();
        restored
            .state_mut()
            .mark_retained("Config.Resource.Program.Count");
        restored.load_retained(&store).unwrap();
        assert_eq!(
            restored.state().read("Config.Resource.Program.Count"),
            Some(Value::Int(9))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn access_runtime_binds_external_io_through_hal() {
        let root = unique_temp("access_io");
        let output_path = root.join("out/value");
        let input_path = root.join("in/value");
        fs::create_dir_all(input_path.parent().unwrap()).unwrap();
        fs::write(&input_path, "1\n").unwrap();
        let mut hal = FileBackedHal::new()
            .bind_location("%IX0.0", &input_path, IoEncoding::Bool01)
            .bind_location("%QX0.0", &output_path, IoEncoding::Bool01);
        let mut runtime = AccessRuntime::new();
        runtime.register(AccessPathBinding::io(
            "StartInput",
            IoSymbol::new("Start", "%IX0.0", IoDirection::Input, "BOOL"),
            AccessDirection::ReadOnly,
        ));
        runtime.register(AccessPathBinding::io(
            "MotorOutput",
            IoSymbol::new("Motor", "%QX0.0", IoDirection::Output, "BOOL"),
            AccessDirection::ReadWrite,
        ));

        assert_eq!(
            runtime.read_access_with_hal("StartInput", &hal),
            Ok(Some(Value::Bool(true)))
        );
        assert_eq!(
            runtime.write_access("MotorOutput", Value::Bool(true)),
            Err(TargetError::MissingIoHal("MotorOutput".to_string()))
        );
        assert_eq!(
            runtime.write_access_with_hal("MotorOutput", Value::Bool(true), &mut hal),
            Ok(true)
        );
        assert_eq!(fs::read_to_string(output_path).unwrap(), "1\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn access_runtime_binds_external_transport_and_retained_state() {
        let mut runtime = AccessRuntime::new();
        runtime.register(AccessPathBinding::io(
            "SpeedLimit",
            IoSymbol::new("SpeedLimit", "", IoDirection::Memory, "INT"),
            AccessDirection::ReadWrite,
        ));
        runtime.register(AccessPathBinding::io(
            "BatteryPercent",
            IoSymbol::new("BatteryPercent", "", IoDirection::Input, "INT"),
            AccessDirection::ReadOnly,
        ));

        let mut bridge = Ros2Bridge::new()
            .bind_name(
                "SpeedLimit",
                Ros2Point::new(Ros2Direction::Parameter, "/robot/speed_limit"),
            )
            .bind_name(
                "BatteryPercent",
                Ros2Point::new(Ros2Direction::Subscribe, "/robot/battery_percent"),
            );
        bridge.set_parameter("/robot/speed_limit", Value::Int(1000));
        bridge.receive("/robot/battery_percent", Value::Int(91));

        assert_eq!(
            runtime.read_access_with_hal("SpeedLimit", &bridge),
            Ok(Some(Value::Int(1000)))
        );
        assert_eq!(
            runtime.read_access_with_hal("BatteryPercent", &bridge),
            Ok(Some(Value::Int(91)))
        );
        assert_eq!(
            runtime.write_access_with_hal("SpeedLimit", Value::Int(750), &mut bridge),
            Ok(true)
        );
        assert_eq!(
            bridge.read(&IoSymbol::new("SpeedLimit", "", IoDirection::Memory, "INT")),
            Some(Value::Int(750))
        );
        assert_eq!(
            runtime.write_access_with_hal("BatteryPercent", Value::Int(0), &mut bridge),
            Err(TargetError::ReadOnlyAccessPath(
                "BatteryPercent".to_string()
            ))
        );
    }

    #[test]
    fn modbus_hal_maps_symbols_to_coils_and_registers() {
        let motor_point = ModbusPoint::new(1, ModbusArea::Coil, 17);
        let count_point = ModbusPoint::new(1, ModbusArea::HoldingRegister, 400);
        let ready_point = ModbusPoint::new(1, ModbusArea::DiscreteInput, 3);
        let temp_point = ModbusPoint::new(1, ModbusArea::InputRegister, 10);
        let mut hal = ModbusHal::new()
            .bind_location("%QX0.0", motor_point)
            .bind_location("%MW0", count_point)
            .bind_name("Ready", ready_point)
            .bind_name("Temperature", temp_point);
        hal.image_mut().set_bit(ready_point, true);
        hal.image_mut().set_register(temp_point, 275);

        let motor = IoSymbol::new("Motor", "%QX0.0", IoDirection::Output, "BOOL");
        let count = IoSymbol::new("Count", "%MW0", IoDirection::Memory, "INT");
        assert!(hal.write(&motor, &Value::Bool(true)));
        assert!(hal.write(&count, &Value::Int(42)));
        assert_eq!(hal.read(&motor), Some(Value::Bool(true)));
        assert_eq!(hal.read(&count), Some(Value::Int(42)));

        let ready = IoSymbol::new("Ready", "", IoDirection::Input, "BOOL");
        let temperature = IoSymbol::new("Temperature", "", IoDirection::Input, "INT");
        assert_eq!(hal.read(&ready), Some(Value::Bool(true)));
        assert_eq!(hal.read(&temperature), Some(Value::Int(275)));
        assert!(!hal.write(&ready, &Value::Bool(false)));
        assert!(!hal.write(&temperature, &Value::Int(0)));
    }

    #[test]
    fn modbus_mapping_loader_parses_unit_area_and_address() {
        let mut hal = load_modbus_mapping(
            r#"
            %QX0.1,1:coil:9
            %MW2,2:holding:12
            Sensor,3:discrete:7
            Analog,3:input:8
            "#,
        );
        let coil = ModbusPoint::new(1, ModbusArea::Coil, 9);
        let holding = ModbusPoint::new(2, ModbusArea::HoldingRegister, 12);
        let discrete = ModbusPoint::new(3, ModbusArea::DiscreteInput, 7);
        let input = ModbusPoint::new(3, ModbusArea::InputRegister, 8);
        hal.image_mut().set_bit(discrete, true);
        hal.image_mut().set_register(input, 1234);

        assert_eq!(ModbusPoint::parse("1:coil:9"), Some(coil));
        assert!(hal.write(
            &IoSymbol::new("Out", "%QX0.1", IoDirection::Output, "BOOL"),
            &Value::Bool(true)
        ));
        assert!(hal.write(
            &IoSymbol::new("Word", "%MW2", IoDirection::Memory, "INT"),
            &Value::Int(99)
        ));
        assert_eq!(
            hal.read(&IoSymbol::new("Out", "%QX0.1", IoDirection::Output, "BOOL")),
            Some(Value::Bool(true))
        );
        assert_eq!(
            hal.read(&IoSymbol::new("Word", "%MW2", IoDirection::Memory, "INT")),
            Some(Value::Int(99))
        );
        assert_eq!(
            hal.read(&IoSymbol::new("Sensor", "", IoDirection::Input, "BOOL")),
            Some(Value::Bool(true))
        );
        assert_eq!(
            hal.read(&IoSymbol::new("Analog", "", IoDirection::Input, "INT")),
            Some(Value::Int(1234))
        );
        assert_eq!(hal.image.read(holding), Value::Int(99));
    }

    #[test]
    fn ethercat_hal_maps_pdo_inputs_and_outputs() {
        let start_point = EtherCatPdoPoint::new(
            1,
            0x6000,
            1,
            EtherCatArea::Input,
            0,
            1,
            EtherCatValueType::Bool,
        );
        let speed_point = EtherCatPdoPoint::new(
            1,
            0x7000,
            1,
            EtherCatArea::Output,
            2,
            0,
            EtherCatValueType::U16,
        );
        let mut hal = EtherCatHal::new(4, 4)
            .bind_name("Start", start_point)
            .bind_location("%QW0", speed_point);
        assert!(hal
            .image_mut()
            .write_input_for_simulation(start_point, &Value::Bool(true)));

        assert_eq!(
            hal.read(&IoSymbol::new("Start", "", IoDirection::Input, "BOOL")),
            Some(Value::Bool(true))
        );
        assert!(hal.write(
            &IoSymbol::new("Speed", "%QW0", IoDirection::Output, "UINT"),
            &Value::Int(1200)
        ));
        assert_eq!(
            hal.read(&IoSymbol::new("Speed", "%QW0", IoDirection::Output, "UINT")),
            Some(Value::Int(1200))
        );
        assert_eq!(&hal.image().output_bytes()[2..4], &1200_u16.to_le_bytes());
        assert!(!hal.write(
            &IoSymbol::new("Start", "", IoDirection::Input, "BOOL"),
            &Value::Bool(false)
        ));
    }

    #[test]
    fn ethercat_mapping_loader_parses_pdo_points() {
        let mut hal = load_ethercat_mapping(
            2,
            6,
            r#"
            Start,1:0x6000:1:input:0.0:bool
            %QD0,2:0x7000:2:output:2.0:i32
            "#,
        );

        assert_eq!(
            EtherCatPdoPoint::parse("1:0x6000:1:input:0.0:bool"),
            Some(EtherCatPdoPoint::new(
                1,
                0x6000,
                1,
                EtherCatArea::Input,
                0,
                0,
                EtherCatValueType::Bool,
            ))
        );
        assert!(hal.image_mut().write_input_for_simulation(
            EtherCatPdoPoint::parse("1:0x6000:1:input:0.0:bool").unwrap(),
            &Value::Bool(true)
        ));
        assert_eq!(
            hal.read(&IoSymbol::new("Start", "", IoDirection::Input, "BOOL")),
            Some(Value::Bool(true))
        );
        assert!(hal.write(
            &IoSymbol::new("DriveCommand", "%QD0", IoDirection::Output, "DINT"),
            &Value::Int(-123)
        ));
        assert_eq!(
            hal.read(&IoSymbol::new(
                "DriveCommand",
                "%QD0",
                IoDirection::Output,
                "DINT"
            )),
            Some(Value::Int(-123))
        );
    }

    #[test]
    fn safety_gate_blocks_outputs_until_enabled_and_reset() {
        let output = IoSymbol::new("Motor", "%QX0.0", IoDirection::Output, "BOOL");
        let memory = IoSymbol::new("Scratch", "%MX0.0", IoDirection::Memory, "BOOL");
        let mut gate = SafetyGate::new(true);

        gate.update(SafetyInputs::default(), false);
        assert_eq!(
            gate.check_output_write(&output),
            Err(SafetyTrip::NotEnabled)
        );
        assert_eq!(gate.check_output_write(&memory), Ok(()));

        gate.update(
            SafetyInputs {
                emergency_stop: true,
                operator_enable: true,
                ..SafetyInputs::default()
            },
            false,
        );
        assert_eq!(
            gate.check_output_write(&output),
            Err(SafetyTrip::EmergencyStop)
        );

        gate.update(
            SafetyInputs {
                operator_enable: true,
                reset: true,
                ..SafetyInputs::default()
            },
            false,
        );
        assert_eq!(gate.check_output_write(&output), Ok(()));

        gate.update(
            SafetyInputs {
                operator_enable: true,
                ..SafetyInputs::default()
            },
            true,
        );
        assert_eq!(
            gate.check_output_write(&output),
            Err(SafetyTrip::WatchdogExpired)
        );
    }

    #[test]
    fn safety_hal_guards_target_output_writes() {
        let point = ModbusPoint::new(1, ModbusArea::Coil, 1);
        let hal = ModbusHal::new().bind_location("%QX0.0", point);
        let mut safe_hal = SafetyHal::new(hal, SafetyGate::new(true));
        let output = IoSymbol::new("Motor", "%QX0.0", IoDirection::Output, "BOOL");

        let blocked = safe_hal
            .write_symbol(&output, &Value::Bool(true))
            .expect_err("operator enable should be required");
        assert_eq!(blocked.kind(), io::ErrorKind::PermissionDenied);

        safe_hal.gate_mut().update(
            SafetyInputs {
                operator_enable: true,
                ..SafetyInputs::default()
            },
            false,
        );
        assert_eq!(
            safe_hal.write_symbol(&output, &Value::Bool(true)).unwrap(),
            true
        );

        let hal = safe_hal.into_inner();
        assert_eq!(hal.image.read(point), Value::Bool(true));
    }

    #[test]
    fn ros2_bridge_maps_subscriptions_publications_and_parameters() {
        let mut bridge = Ros2Bridge::new()
            .bind_name(
                "BatteryPercent",
                Ros2Point::new(Ros2Direction::Subscribe, "/robot/battery"),
            )
            .bind_location(
                "%QX0.0",
                Ros2Point::new(Ros2Direction::Publish, "/robot/motor_enable"),
            )
            .bind_name(
                "MaxSpeed",
                Ros2Point::new(Ros2Direction::Parameter, "/robot/max_speed"),
            );

        bridge.receive("/robot/battery", Value::Int(87));
        bridge.set_parameter("/robot/max_speed", Value::Int(1200));
        assert_eq!(
            bridge.read(&IoSymbol::new(
                "BatteryPercent",
                "",
                IoDirection::Input,
                "INT"
            )),
            Some(Value::Int(87))
        );
        assert!(bridge.write(
            &IoSymbol::new("MotorEnable", "%QX0.0", IoDirection::Output, "BOOL"),
            &Value::Bool(true)
        ));
        assert_eq!(
            bridge.published(),
            &[Ros2Message {
                topic: "/robot/motor_enable".to_string(),
                value: Value::Bool(true),
            }]
        );
        assert_eq!(
            bridge.write(
                &IoSymbol::new("BatteryPercent", "", IoDirection::Input, "INT"),
                &Value::Int(0)
            ),
            false
        );
        assert_eq!(
            bridge.read(&IoSymbol::new("MaxSpeed", "", IoDirection::Memory, "INT")),
            Some(Value::Int(1200))
        );
        assert!(bridge.write(
            &IoSymbol::new("MaxSpeed", "", IoDirection::Memory, "INT"),
            &Value::Int(900)
        ));
        assert_eq!(
            bridge.read(&IoSymbol::new("MaxSpeed", "", IoDirection::Memory, "INT")),
            Some(Value::Int(900))
        );
    }

    #[test]
    fn ros2_mapping_loader_parses_topics() {
        let mut bridge = load_ros2_mapping(
            r#"
            Battery,sub:/battery
            %QX0.1,pub:/motor_enable
            Speed,param:/speed_limit
            "#,
        );
        assert_eq!(
            Ros2Point::parse("pub:/motor_enable"),
            Some(Ros2Point::new(Ros2Direction::Publish, "/motor_enable"))
        );
        bridge.receive("/battery", Value::Int(64));
        assert_eq!(
            bridge.read(&IoSymbol::new("Battery", "", IoDirection::Input, "INT")),
            Some(Value::Int(64))
        );
        assert!(bridge.write(
            &IoSymbol::new("Motor", "%QX0.1", IoDirection::Output, "BOOL"),
            &Value::Bool(true)
        ));
        assert_eq!(bridge.take_published().len(), 1);
        assert!(bridge.write(
            &IoSymbol::new("Speed", "", IoDirection::Memory, "INT"),
            &Value::Int(250)
        ));
        assert_eq!(
            bridge.read(&IoSymbol::new("Speed", "", IoDirection::Memory, "INT")),
            Some(Value::Int(250))
        );
    }

    #[test]
    fn modbus_transport_hal_uses_transport_trait() {
        let motor_point = ModbusPoint::new(1, ModbusArea::Coil, 9);
        let ready_point = ModbusPoint::new(1, ModbusArea::DiscreteInput, 3);
        let speed_point = ModbusPoint::new(1, ModbusArea::HoldingRegister, 10);
        let analog_point = ModbusPoint::new(1, ModbusArea::InputRegister, 11);
        let mut image = ModbusImage::default();
        image.set_bit(ready_point, true);
        image.set_register(analog_point, 275);

        let mut hal = ModbusTransportHal::new(image)
            .bind_location("%QX0.0", motor_point)
            .bind_name("Ready", ready_point)
            .bind_name("Speed", speed_point)
            .bind_name("Analog", analog_point);

        assert_eq!(
            hal.read_symbol(&IoSymbol::new("Ready", "", IoDirection::Input, "BOOL"))
                .unwrap(),
            Some(Value::Bool(true))
        );
        assert_eq!(
            hal.read_symbol(&IoSymbol::new("Analog", "", IoDirection::Input, "INT"))
                .unwrap(),
            Some(Value::Int(275))
        );
        assert!(hal
            .write_symbol(
                &IoSymbol::new("Motor", "%QX0.0", IoDirection::Output, "BOOL"),
                &Value::Bool(true),
            )
            .unwrap());
        assert!(hal
            .write_symbol(
                &IoSymbol::new("Speed", "", IoDirection::Memory, "UINT"),
                &Value::Int(1200),
            )
            .unwrap());
        assert!(hal.transport().read_coil(1, 9).unwrap());
        assert_eq!(hal.transport().read_holding_register(1, 10).unwrap(), 1200);
        assert!(!hal
            .write_symbol(
                &IoSymbol::new("Ready", "", IoDirection::Input, "BOOL"),
                &Value::Bool(false),
            )
            .unwrap());
    }

    #[test]
    fn ethercat_transport_hal_reads_and_writes_pdo_segments() {
        let start_point = EtherCatPdoPoint::new(
            1,
            0x6000,
            1,
            EtherCatArea::Input,
            0,
            2,
            EtherCatValueType::Bool,
        );
        let speed_point = EtherCatPdoPoint::new(
            1,
            0x7000,
            1,
            EtherCatArea::Output,
            2,
            0,
            EtherCatValueType::U16,
        );
        let mut image = EtherCatPdoImage::new(2, 4);
        assert!(image.write_input_for_simulation(start_point, &Value::Bool(true)));

        let mut hal = EtherCatTransportHal::new(image)
            .bind_name("Start", start_point)
            .bind_location("%QW0", speed_point);

        assert_eq!(
            hal.read_symbol(&IoSymbol::new("Start", "", IoDirection::Input, "BOOL"))
                .unwrap(),
            Some(Value::Bool(true))
        );
        assert!(hal
            .write_symbol(
                &IoSymbol::new("Speed", "%QW0", IoDirection::Output, "UINT"),
                &Value::Int(1200),
            )
            .unwrap());
        assert_eq!(
            &hal.transport().output_bytes()[2..4],
            &1200_u16.to_le_bytes()
        );
        assert!(!hal
            .write_symbol(
                &IoSymbol::new("Start", "", IoDirection::Input, "BOOL"),
                &Value::Bool(false),
            )
            .unwrap());
    }

    #[test]
    fn ros2_transport_hal_maps_topics_and_parameters() {
        let mut bridge = Ros2Bridge::new();
        bridge.receive("/battery", Value::Int(88));
        bridge.set_parameter("/speed_limit", Value::Int(1000));
        let mut hal = Ros2TransportHal::new(bridge)
            .bind_name(
                "Battery",
                Ros2Point::new(Ros2Direction::Subscribe, "/battery"),
            )
            .bind_name(
                "MotorEnable",
                Ros2Point::new(Ros2Direction::Publish, "/motor_enable"),
            )
            .bind_name(
                "SpeedLimit",
                Ros2Point::new(Ros2Direction::Parameter, "/speed_limit"),
            );

        assert_eq!(
            hal.read_symbol(&IoSymbol::new("Battery", "", IoDirection::Input, "INT"))
                .unwrap(),
            Some(Value::Int(88))
        );
        assert_eq!(
            hal.read_symbol(&IoSymbol::new("SpeedLimit", "", IoDirection::Memory, "INT"))
                .unwrap(),
            Some(Value::Int(1000))
        );
        assert!(hal
            .write_symbol(
                &IoSymbol::new("MotorEnable", "", IoDirection::Output, "BOOL"),
                &Value::Bool(true),
            )
            .unwrap());
        assert!(hal
            .write_symbol(
                &IoSymbol::new("SpeedLimit", "", IoDirection::Memory, "INT"),
                &Value::Int(750),
            )
            .unwrap());
        assert_eq!(
            hal.transport().published(),
            &[Ros2Message {
                topic: "/motor_enable".to_string(),
                value: Value::Bool(true),
            }]
        );
        assert_eq!(
            hal.transport().read_parameter("/speed_limit").unwrap(),
            Some(Value::Int(750))
        );
    }

    #[test]
    fn target_supervisor_coordinates_retain_watchdog_and_cycle_reports() {
        let root = unique_temp("supervisor_retain");
        let store = RetainStore::new(&root);
        store.save("Robot.Count", &Value::Int(3)).unwrap();
        let mut access = AccessRuntime::new();
        access.register(AccessPathBinding::state(
            "Count",
            "Robot.Count",
            AccessDirection::ReadWrite,
            "DINT",
        ));
        access.state_mut().set("Robot.Count", Value::Int(0));
        access.state_mut().mark_retained("Robot.Count");

        let mut supervisor = TargetSupervisor::new(ModbusHal::new())
            .with_access_runtime(access)
            .with_retain_store(store.clone())
            .with_watchdog(CycleWatchdog::new(Duration::from_secs(1)));
        supervisor.cold_start().unwrap();
        assert_eq!(
            supervisor.access().state().read("Robot.Count"),
            Some(Value::Int(3))
        );

        supervisor.begin_cycle();
        assert_eq!(supervisor.write_access("Count", Value::Int(9)), Ok(true));
        let report = supervisor.end_cycle().unwrap();
        assert_eq!(report.cycle, 1);
        assert!(report.retained_saved);
        assert!(!report.watchdog_expired);
        assert_eq!(supervisor.cycle_count(), 1);
        assert_eq!(store.load("Robot.Count").unwrap(), Some(Value::Int(9)));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn target_supervisor_routes_io_and_blocks_unsafe_outputs() {
        let start_point = ModbusPoint::new(1, ModbusArea::DiscreteInput, 1);
        let motor_point = ModbusPoint::new(1, ModbusArea::Coil, 2);
        let mut image = ModbusImage::default();
        image.set_bit(start_point, true);
        let hal = ModbusTransportHal::new(image)
            .bind_name("Start", start_point)
            .bind_name("Motor", motor_point);
        let mut access = AccessRuntime::new();
        access.register(AccessPathBinding::io(
            "StartInput",
            IoSymbol::new("Start", "", IoDirection::Input, "BOOL"),
            AccessDirection::ReadOnly,
        ));
        access.register(AccessPathBinding::io(
            "MotorOutput",
            IoSymbol::new("Motor", "", IoDirection::Output, "BOOL"),
            AccessDirection::ReadWrite,
        ));
        let mut supervisor = TargetSupervisor::new(hal)
            .with_access_runtime(access)
            .with_watchdog(CycleWatchdog::new(Duration::from_millis(5)))
            .with_safety_gate(SafetyGate::new(true));
        supervisor.cold_start().unwrap();

        supervisor.begin_cycle();
        assert_eq!(
            supervisor.read_access("StartInput"),
            Ok(Some(Value::Bool(true)))
        );
        assert_eq!(
            supervisor.write_access("MotorOutput", Value::Bool(true)),
            Err(TargetError::Safety {
                name: "MotorOutput".to_string(),
                reason: SafetyTrip::NotEnabled,
            })
        );

        supervisor.set_safety_inputs(SafetyInputs {
            operator_enable: true,
            reset: true,
            ..SafetyInputs::default()
        });
        supervisor.begin_cycle();
        assert_eq!(
            supervisor.write_access("MotorOutput", Value::Bool(true)),
            Ok(true)
        );
        assert!(!supervisor.end_cycle().unwrap().watchdog_expired);

        std::thread::sleep(Duration::from_millis(10));
        supervisor.begin_cycle();
        assert_eq!(
            supervisor.write_access("MotorOutput", Value::Bool(false)),
            Err(TargetError::Safety {
                name: "MotorOutput".to_string(),
                reason: SafetyTrip::WatchdogExpired,
            })
        );
        let report = supervisor.end_cycle().unwrap();
        assert!(report.watchdog_expired);
        assert_eq!(report.safety_state.unwrap().outputs_enabled, false);
    }

    #[test]
    fn cycle_watchdog_reports_expiry_after_deadline() {
        let mut watchdog = CycleWatchdog::new(Duration::from_millis(20));
        assert_eq!(watchdog.timeout(), Duration::from_millis(20));
        watchdog.pet();
        assert!(!watchdog.expired());
        std::thread::sleep(Duration::from_millis(30));
        assert!(watchdog.expired());
    }
}
