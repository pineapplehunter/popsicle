use futures::StreamExt;
// abortable
use gtk::glib;
use std::{cell::RefCell, collections::HashMap, sync::Arc};
use zbus::{dbus_proxy, fdo::ObjectManagerProxy};

// TODO: make it a gobject, with signals for added/removed device? Or change a `changed` signal
struct Disks {
    connection: zbus::Connection,
    object_manager: ObjectManagerProxy<'static>,
    disks: RefCell<HashMap<zbus::zvariant::OwnedObjectPath, Arc<DiskDevice>>>,
    blocks: RefCell<HashMap<zbus::zvariant::OwnedObjectPath, BlockProxy<'static>>>,
}

impl Disks {
    async fn new(&self) -> zbus::Result<Self> {
        let connection = zbus::Connection::system().await?;
        let object_manager = ObjectManagerProxy::builder(&connection)
            .destination("org.freedesktop.UDisks2")?
            .build()
            .await?;
        let mut added_stream = object_manager.receive_interfaces_added().await?;
        let mut removed_stream = object_manager.receive_interfaces_removed().await?;
        glib::MainContext::default().spawn(async move {
            // TODO self?
            while let Some(evt) = added_stream.next().await {
                if let Ok(args) = evt.args() {
                    let interfaces = &args.interfaces_and_properties;
                    if interfaces.contains_key("org.freedesktop.UDisks2.Drive") {
                        self.new_drive(args.object_path.into());
                    }
                    if interfaces.contains_key("org.freedesktop.UDisks2.Block") {
                        self.new_block(args.object_path.into());
                    }
                }
            }
        });
        for (object_path, interfaces) in object_manager.get_managed_objects().await?.iter() {
            if interfaces.contains_key("org.freedesktop.UDisks2.Drive") {
                self.new_drive(object_path.clone());
            }
            if interfaces.contains_key("org.freedesktop.UDisks2.Block") {
                self.new_block(object_path.clone());
            }
        }
        Ok(Self { connection, object_manager, disks: Default::default(), blocks: Default::default() })
    }

    async fn new_drive(&self, path: zbus::zvariant::OwnedObjectPath) -> zbus::Result<DriveProxy<'static>> {
        Ok(DriveProxy::builder(&self.connection)
            .destination("org.freedesktop.UDisks2")?
            .path(path)?
            .cache_properties(zbus::CacheProperties::Yes)
            .build()
            .await?)
    }

    async fn new_block(&self, path: zbus::zvariant::OwnedObjectPath) -> zbus::Result<()> {
        let block = BlockProxy::builder(&self.connection)
            .destination("org.freedesktop.UDisks2")?
            .path(path)?
            .cache_properties(zbus::CacheProperties::Yes)
            .build()
            .await?;
        // XXX correct to assume drive created first? Parent block?
        let drive = block.cached_drive()?.unwrap();
        Ok(())
    }

    fn disks(&self) -> Vec<Arc<DiskDevice>> {
        let disks = self.disks.borrow();
        disks.values().cloned().collect()
    }
}

// TODO: what if this changes, and there aren't as many partitions?
struct DiskDevice {
    pub drive: DriveProxy<'static>,
    pub parent: BlockProxy<'static>,
    pub partitions: Vec<RefCell<BlockProxy<'static>>>,
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Manager")]
trait Manager {
    /// CanCheck method
    fn can_check(&self, type_: &str) -> zbus::Result<(bool, String)>;

    /// CanFormat method
    fn can_format(&self, type_: &str) -> zbus::Result<(bool, String)>;

    /// CanRepair method
    fn can_repair(&self, type_: &str) -> zbus::Result<(bool, String)>;

    /// CanResize method
    fn can_resize(&self, type_: &str) -> zbus::Result<(bool, u64, String)>;

    /// EnableModule method
    fn enable_module(&self, name: &str, enable: bool) -> zbus::Result<()>;

    /// EnableModules method
    fn enable_modules(&self, enable: bool) -> zbus::Result<()>;

    /// GetBlockDevices method
    fn get_block_devices(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// LoopSetup method
    fn loop_setup(
        &self,
        fd: zbus::zvariant::Fd,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// MDRaidCreate method
    fn mdraid_create(
        &self,
        blocks: &[zbus::zvariant::ObjectPath<'_>],
        level: &str,
        name: &str,
        chunk: u64,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// ResolveDevice method
    fn resolve_device(
        &self,
        devspec: HashMap<&str, zbus::zvariant::Value<'_>>,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// DefaultEncryptionType property
    #[dbus_proxy(property)]
    fn default_encryption_type(&self) -> zbus::Result<String>;

    /// SupportedEncryptionTypes property
    #[dbus_proxy(property)]
    fn supported_encryption_types(&self) -> zbus::Result<Vec<String>>;

    /// SupportedFilesystems property
    #[dbus_proxy(property)]
    fn supported_filesystems(&self) -> zbus::Result<Vec<String>>;

    /// Version property
    #[dbus_proxy(property)]
    fn version(&self) -> zbus::Result<String>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Drive")]
trait Drive {
    /// Eject method
    fn eject(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// PowerOff method
    fn power_off(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// SetConfiguration method
    fn set_configuration(
        &self,
        value: HashMap<&str, zbus::zvariant::Value<'_>>,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// CanPowerOff property
    #[dbus_proxy(property)]
    fn can_power_off(&self) -> zbus::Result<bool>;

    /// Configuration property
    #[dbus_proxy(property)]
    fn configuration(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;

    /// ConnectionBus property
    #[dbus_proxy(property)]
    fn connection_bus(&self) -> zbus::Result<String>;

    /// Ejectable property
    #[dbus_proxy(property)]
    fn ejectable(&self) -> zbus::Result<bool>;

    /// Id property
    #[dbus_proxy(property)]
    fn id(&self) -> zbus::Result<String>;

    /// Media property
    #[dbus_proxy(property)]
    fn media(&self) -> zbus::Result<String>;

    /// MediaAvailable property
    #[dbus_proxy(property)]
    fn media_available(&self) -> zbus::Result<bool>;

    /// MediaChangeDetected property
    #[dbus_proxy(property)]
    fn media_change_detected(&self) -> zbus::Result<bool>;

    /// MediaCompatibility property
    #[dbus_proxy(property)]
    fn media_compatibility(&self) -> zbus::Result<Vec<String>>;

    /// MediaRemovable property
    #[dbus_proxy(property)]
    fn media_removable(&self) -> zbus::Result<bool>;

    /// Model property
    #[dbus_proxy(property)]
    fn model(&self) -> zbus::Result<String>;

    /// Optical property
    #[dbus_proxy(property)]
    fn optical(&self) -> zbus::Result<bool>;

    /// OpticalBlank property
    #[dbus_proxy(property)]
    fn optical_blank(&self) -> zbus::Result<bool>;

    /// OpticalNumAudioTracks property
    #[dbus_proxy(property)]
    fn optical_num_audio_tracks(&self) -> zbus::Result<u32>;

    /// OpticalNumDataTracks property
    #[dbus_proxy(property)]
    fn optical_num_data_tracks(&self) -> zbus::Result<u32>;

    /// OpticalNumSessions property
    #[dbus_proxy(property)]
    fn optical_num_sessions(&self) -> zbus::Result<u32>;

    /// OpticalNumTracks property
    #[dbus_proxy(property)]
    fn optical_num_tracks(&self) -> zbus::Result<u32>;

    /// Removable property
    #[dbus_proxy(property)]
    fn removable(&self) -> zbus::Result<bool>;

    /// Revision property
    #[dbus_proxy(property)]
    fn revision(&self) -> zbus::Result<String>;

    /// RotationRate property
    #[dbus_proxy(property)]
    fn rotation_rate(&self) -> zbus::Result<i32>;

    /// Seat property
    #[dbus_proxy(property)]
    fn seat(&self) -> zbus::Result<String>;

    /// Serial property
    #[dbus_proxy(property)]
    fn serial(&self) -> zbus::Result<String>;

    /// SiblingId property
    #[dbus_proxy(property)]
    fn sibling_id(&self) -> zbus::Result<String>;

    /// Size property
    #[dbus_proxy(property)]
    fn size(&self) -> zbus::Result<u64>;

    /// SortKey property
    #[dbus_proxy(property)]
    fn sort_key(&self) -> zbus::Result<String>;

    /// TimeDetected property
    #[dbus_proxy(property)]
    fn time_detected(&self) -> zbus::Result<u64>;

    /// TimeMediaDetected property
    #[dbus_proxy(property)]
    fn time_media_detected(&self) -> zbus::Result<u64>;

    /// Vendor property
    #[dbus_proxy(property)]
    fn vendor(&self) -> zbus::Result<String>;

    /// WWN property
    #[dbus_proxy(property)]
    fn wwn(&self) -> zbus::Result<String>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Drive.Ata")]
trait Ata {
    /// PmGetState method
    fn pm_get_state(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<u8>;

    /// PmStandby method
    fn pm_standby(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// PmWakeup method
    fn pm_wakeup(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// SecurityEraseUnit method
    fn security_erase_unit(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SmartGetAttributes method
    fn smart_get_attributes(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<
        Vec<(
            u8,
            String,
            u16,
            i32,
            i32,
            i32,
            i64,
            i32,
            HashMap<String, zbus::zvariant::OwnedValue>,
        )>,
    >;

    /// SmartSelftestAbort method
    fn smart_selftest_abort(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SmartSelftestStart method
    fn smart_selftest_start(
        &self,
        type_: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SmartSetEnabled method
    fn smart_set_enabled(
        &self,
        value: bool,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SmartUpdate method
    fn smart_update(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// AamEnabled property
    #[dbus_proxy(property)]
    fn aam_enabled(&self) -> zbus::Result<bool>;

    /// AamSupported property
    #[dbus_proxy(property)]
    fn aam_supported(&self) -> zbus::Result<bool>;

    /// AamVendorRecommendedValue property
    #[dbus_proxy(property)]
    fn aam_vendor_recommended_value(&self) -> zbus::Result<i32>;

    /// ApmEnabled property
    #[dbus_proxy(property)]
    fn apm_enabled(&self) -> zbus::Result<bool>;

    /// ApmSupported property
    #[dbus_proxy(property)]
    fn apm_supported(&self) -> zbus::Result<bool>;

    /// PmEnabled property
    #[dbus_proxy(property)]
    fn pm_enabled(&self) -> zbus::Result<bool>;

    /// PmSupported property
    #[dbus_proxy(property)]
    fn pm_supported(&self) -> zbus::Result<bool>;

    /// ReadLookaheadEnabled property
    #[dbus_proxy(property)]
    fn read_lookahead_enabled(&self) -> zbus::Result<bool>;

    /// ReadLookaheadSupported property
    #[dbus_proxy(property)]
    fn read_lookahead_supported(&self) -> zbus::Result<bool>;

    /// SecurityEnhancedEraseUnitMinutes property
    #[dbus_proxy(property)]
    fn security_enhanced_erase_unit_minutes(&self) -> zbus::Result<i32>;

    /// SecurityEraseUnitMinutes property
    #[dbus_proxy(property)]
    fn security_erase_unit_minutes(&self) -> zbus::Result<i32>;

    /// SecurityFrozen property
    #[dbus_proxy(property)]
    fn security_frozen(&self) -> zbus::Result<bool>;

    /// SmartEnabled property
    #[dbus_proxy(property)]
    fn smart_enabled(&self) -> zbus::Result<bool>;

    /// SmartFailing property
    #[dbus_proxy(property)]
    fn smart_failing(&self) -> zbus::Result<bool>;

    /// SmartNumAttributesFailedInThePast property
    #[dbus_proxy(property)]
    fn smart_num_attributes_failed_in_the_past(&self) -> zbus::Result<i32>;

    /// SmartNumAttributesFailing property
    #[dbus_proxy(property)]
    fn smart_num_attributes_failing(&self) -> zbus::Result<i32>;

    /// SmartNumBadSectors property
    #[dbus_proxy(property)]
    fn smart_num_bad_sectors(&self) -> zbus::Result<i64>;

    /// SmartPowerOnSeconds property
    #[dbus_proxy(property)]
    fn smart_power_on_seconds(&self) -> zbus::Result<u64>;

    /// SmartSelftestPercentRemaining property
    #[dbus_proxy(property)]
    fn smart_selftest_percent_remaining(&self) -> zbus::Result<i32>;

    /// SmartSelftestStatus property
    #[dbus_proxy(property)]
    fn smart_selftest_status(&self) -> zbus::Result<String>;

    /// SmartSupported property
    #[dbus_proxy(property)]
    fn smart_supported(&self) -> zbus::Result<bool>;

    /// SmartTemperature property
    #[dbus_proxy(property)]
    fn smart_temperature(&self) -> zbus::Result<f64>;

    /// SmartUpdated property
    #[dbus_proxy(property)]
    fn smart_updated(&self) -> zbus::Result<u64>;

    /// WriteCacheEnabled property
    #[dbus_proxy(property)]
    fn write_cache_enabled(&self) -> zbus::Result<bool>;

    /// WriteCacheSupported property
    #[dbus_proxy(property)]
    fn write_cache_supported(&self) -> zbus::Result<bool>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Block")]
trait Block {
    /// AddConfigurationItem method
    fn add_configuration_item(
        &self,
        item: &(&str, HashMap<&str, zbus::zvariant::Value<'_>>),
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Format method
    fn format(
        &self,
        type_: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// GetSecretConfiguration method
    fn get_secret_configuration(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<Vec<(String, HashMap<String, zbus::zvariant::OwnedValue>)>>;

    /// OpenDevice method
    fn open_device(
        &self,
        mode: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedFd>;

    /// OpenForBackup method
    fn open_for_backup(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedFd>;

    /// OpenForBenchmark method
    fn open_for_benchmark(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedFd>;

    /// OpenForRestore method
    fn open_for_restore(
        &self,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedFd>;

    /// RemoveConfigurationItem method
    fn remove_configuration_item(
        &self,
        item: &(&str, HashMap<&str, zbus::zvariant::Value<'_>>),
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Rescan method
    fn rescan(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// UpdateConfigurationItem method
    fn update_configuration_item(
        &self,
        old_item: &(&str, HashMap<&str, zbus::zvariant::Value<'_>>),
        new_item: &(&str, HashMap<&str, zbus::zvariant::Value<'_>>),
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Configuration property
    #[dbus_proxy(property)]
    fn configuration(
        &self,
    ) -> zbus::Result<Vec<(String, HashMap<String, zbus::zvariant::OwnedValue>)>>;

    /// CryptoBackingDevice property
    #[dbus_proxy(property)]
    fn crypto_backing_device(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Device property
    #[dbus_proxy(property)]
    fn device(&self) -> zbus::Result<Vec<u8>>;

    /// DeviceNumber property
    #[dbus_proxy(property)]
    fn device_number(&self) -> zbus::Result<u64>;

    /// Drive property
    #[dbus_proxy(property)]
    fn drive(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// HintAuto property
    #[dbus_proxy(property)]
    fn hint_auto(&self) -> zbus::Result<bool>;

    /// HintIconName property
    #[dbus_proxy(property)]
    fn hint_icon_name(&self) -> zbus::Result<String>;

    /// HintIgnore property
    #[dbus_proxy(property)]
    fn hint_ignore(&self) -> zbus::Result<bool>;

    /// HintName property
    #[dbus_proxy(property)]
    fn hint_name(&self) -> zbus::Result<String>;

    /// HintPartitionable property
    #[dbus_proxy(property)]
    fn hint_partitionable(&self) -> zbus::Result<bool>;

    /// HintSymbolicIconName property
    #[dbus_proxy(property)]
    fn hint_symbolic_icon_name(&self) -> zbus::Result<String>;

    /// HintSystem property
    #[dbus_proxy(property)]
    fn hint_system(&self) -> zbus::Result<bool>;

    /// Id property
    #[dbus_proxy(property)]
    fn id(&self) -> zbus::Result<String>;

    /// IdLabel property
    #[dbus_proxy(property)]
    fn id_label(&self) -> zbus::Result<String>;

    /// IdType property
    #[dbus_proxy(property)]
    fn id_type(&self) -> zbus::Result<String>;

    /// IdUUID property
    #[dbus_proxy(property)]
    fn id_uuid(&self) -> zbus::Result<String>;

    /// IdUsage property
    #[dbus_proxy(property)]
    fn id_usage(&self) -> zbus::Result<String>;

    /// IdVersion property
    #[dbus_proxy(property)]
    fn id_version(&self) -> zbus::Result<String>;

    /// MDRaid property
    #[dbus_proxy(property)]
    fn mdraid(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// MDRaidMember property
    #[dbus_proxy(property)]
    fn mdraid_member(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// PreferredDevice property
    #[dbus_proxy(property)]
    fn preferred_device(&self) -> zbus::Result<Vec<u8>>;

    /// ReadOnly property
    #[dbus_proxy(property)]
    fn read_only(&self) -> zbus::Result<bool>;

    /// Size property
    #[dbus_proxy(property)]
    fn size(&self) -> zbus::Result<u64>;

    /// Symlinks property
    #[dbus_proxy(property)]
    fn symlinks(&self) -> zbus::Result<Vec<Vec<u8>>>;

    /// UserspaceMountOptions property
    #[dbus_proxy(property)]
    fn userspace_mount_options(&self) -> zbus::Result<Vec<String>>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.PartitionTable")]
trait PartitionTable {
    /// CreatePartition method
    fn create_partition(
        &self,
        offset: u64,
        size: u64,
        type_: &str,
        name: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// CreatePartitionAndFormat method
    fn create_partition_and_format(
        &self,
        offset: u64,
        size: u64,
        type_: &str,
        name: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
        format_type: &str,
        format_options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Partitions property
    #[dbus_proxy(property)]
    fn partitions(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// Type property
    #[dbus_proxy(property)]
    fn type_(&self) -> zbus::Result<String>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Partition")]
trait Partition {
    /// Delete method
    fn delete(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// Resize method
    fn resize(
        &self,
        size: u64,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SetFlags method
    fn set_flags(
        &self,
        flags: u64,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SetName method
    fn set_name(
        &self,
        name: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SetType method
    fn set_type(
        &self,
        type_: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Flags property
    #[dbus_proxy(property)]
    fn flags(&self) -> zbus::Result<u64>;

    /// IsContained property
    #[dbus_proxy(property)]
    fn is_contained(&self) -> zbus::Result<bool>;

    /// IsContainer property
    #[dbus_proxy(property)]
    fn is_container(&self) -> zbus::Result<bool>;

    /// Name property
    #[dbus_proxy(property)]
    fn name(&self) -> zbus::Result<String>;

    /// Number property
    #[dbus_proxy(property)]
    fn number(&self) -> zbus::Result<u32>;

    /// Offset property
    #[dbus_proxy(property)]
    fn offset(&self) -> zbus::Result<u64>;

    /// Size property
    #[dbus_proxy(property)]
    fn size(&self) -> zbus::Result<u64>;

    /// Table property
    #[dbus_proxy(property)]
    fn table(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Type property
    #[dbus_proxy(property)]
    fn type_(&self) -> zbus::Result<String>;

    /// UUID property
    #[dbus_proxy(property)]
    fn uuid(&self) -> zbus::Result<String>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Filesystem")]
trait Filesystem {
    /// Check method
    fn check(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<bool>;

    /// Mount method
    fn mount(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<String>;

    /// Repair method
    fn repair(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<bool>;

    /// Resize method
    fn resize(
        &self,
        size: u64,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SetLabel method
    fn set_label(
        &self,
        label: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// TakeOwnership method
    fn take_ownership(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>)
        -> zbus::Result<()>;

    /// Unmount method
    fn unmount(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// MountPoints property
    #[dbus_proxy(property)]
    fn mount_points(&self) -> zbus::Result<Vec<Vec<u8>>>;

    /// Size property
    #[dbus_proxy(property)]
    fn size(&self) -> zbus::Result<u64>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Swapspace")]
trait Swapspace {
    /// SetLabel method
    fn set_label(
        &self,
        label: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Start method
    fn start(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// Stop method
    fn stop(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// Active property
    #[dbus_proxy(property)]
    fn active(&self) -> zbus::Result<bool>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Encrypted")]
trait Encrypted {
    /// ChangePassphrase method
    fn change_passphrase(
        &self,
        passphrase: &str,
        new_passphrase: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Lock method
    fn lock(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// Resize method
    fn resize(
        &self,
        size: u64,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Unlock method
    fn unlock(
        &self,
        passphrase: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// ChildConfiguration property
    #[dbus_proxy(property)]
    fn child_configuration(
        &self,
    ) -> zbus::Result<Vec<(String, HashMap<String, zbus::zvariant::OwnedValue>)>>;

    /// CleartextDevice property
    #[dbus_proxy(property)]
    fn cleartext_device(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// HintEncryptionType property
    #[dbus_proxy(property)]
    fn hint_encryption_type(&self) -> zbus::Result<String>;

    /// MetadataSize property
    #[dbus_proxy(property)]
    fn metadata_size(&self) -> zbus::Result<u64>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Loop")]
trait Loop {
    /// Delete method
    fn delete(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// SetAutoclear method
    fn set_autoclear(
        &self,
        value: bool,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Autoclear property
    #[dbus_proxy(property)]
    fn autoclear(&self) -> zbus::Result<bool>;

    /// BackingFile property
    #[dbus_proxy(property)]
    fn backing_file(&self) -> zbus::Result<Vec<u8>>;

    /// SetupByUID property
    #[dbus_proxy(property)]
    fn setup_by_uid(&self) -> zbus::Result<u32>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.MDRaid")]
trait MDRaid {
    /// AddDevice method
    fn add_device(
        &self,
        device: &zbus::zvariant::ObjectPath<'_>,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Delete method
    fn delete(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// RemoveDevice method
    fn remove_device(
        &self,
        device: &zbus::zvariant::ObjectPath<'_>,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// RequestSyncAction method
    fn request_sync_action(
        &self,
        sync_action: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// SetBitmapLocation method
    fn set_bitmap_location(
        &self,
        value: &[u8],
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Start method
    fn start(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// Stop method
    fn stop(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// ActiveDevices property
    #[dbus_proxy(property)]
    fn active_devices(
        &self,
    ) -> zbus::Result<
        Vec<(
            zbus::zvariant::OwnedObjectPath,
            i32,
            Vec<String>,
            u64,
            HashMap<String, zbus::zvariant::OwnedValue>,
        )>,
    >;

    /// BitmapLocation property
    #[dbus_proxy(property)]
    fn bitmap_location(&self) -> zbus::Result<Vec<u8>>;

    /// ChildConfiguration property
    #[dbus_proxy(property)]
    fn child_configuration(
        &self,
    ) -> zbus::Result<Vec<(String, HashMap<String, zbus::zvariant::OwnedValue>)>>;

    /// ChunkSize property
    #[dbus_proxy(property)]
    fn chunk_size(&self) -> zbus::Result<u64>;

    /// Degraded property
    #[dbus_proxy(property)]
    fn degraded(&self) -> zbus::Result<u32>;

    /// Level property
    #[dbus_proxy(property)]
    fn level(&self) -> zbus::Result<String>;

    /// Name property
    #[dbus_proxy(property)]
    fn name(&self) -> zbus::Result<String>;

    /// NumDevices property
    #[dbus_proxy(property)]
    fn num_devices(&self) -> zbus::Result<u32>;

    /// Running property
    #[dbus_proxy(property)]
    fn running(&self) -> zbus::Result<bool>;

    /// Size property
    #[dbus_proxy(property)]
    fn size(&self) -> zbus::Result<u64>;

    /// SyncAction property
    #[dbus_proxy(property)]
    fn sync_action(&self) -> zbus::Result<String>;

    /// SyncCompleted property
    #[dbus_proxy(property)]
    fn sync_completed(&self) -> zbus::Result<f64>;

    /// SyncRate property
    #[dbus_proxy(property)]
    fn sync_rate(&self) -> zbus::Result<u64>;

    /// SyncRemainingTime property
    #[dbus_proxy(property)]
    fn sync_remaining_time(&self) -> zbus::Result<u64>;

    /// UUID property
    #[dbus_proxy(property)]
    fn uuid(&self) -> zbus::Result<String>;
}

#[dbus_proxy(interface = "org.freedesktop.UDisks2.Job")]
trait Job {
    /// Cancel method
    fn cancel(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;

    /// Completed signal
    #[dbus_proxy(signal)]
    fn completed(&self, success: bool, message: &str) -> zbus::Result<()>;

    /// Bytes property
    #[dbus_proxy(property)]
    fn bytes(&self) -> zbus::Result<u64>;

    /// Cancelable property
    #[dbus_proxy(property)]
    fn cancelable(&self) -> zbus::Result<bool>;

    /// ExpectedEndTime property
    #[dbus_proxy(property)]
    fn expected_end_time(&self) -> zbus::Result<u64>;

    /// Objects property
    #[dbus_proxy(property)]
    fn objects(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// Operation property
    #[dbus_proxy(property)]
    fn operation(&self) -> zbus::Result<String>;

    /// Progress property
    #[dbus_proxy(property)]
    fn progress(&self) -> zbus::Result<f64>;

    /// ProgressValid property
    #[dbus_proxy(property)]
    fn progress_valid(&self) -> zbus::Result<bool>;

    /// Rate property
    #[dbus_proxy(property)]
    fn rate(&self) -> zbus::Result<u64>;

    /// StartTime property
    #[dbus_proxy(property)]
    fn start_time(&self) -> zbus::Result<u64>;

    /// StartedByUID property
    #[dbus_proxy(property)]
    fn started_by_uid(&self) -> zbus::Result<u32>;
}
