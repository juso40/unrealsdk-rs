//! Curated UE3 flag and enum sets for the WILLOW games (BL1/BL1/BL2/TPS/AoDK).
//!
//! https://github.com/CodeRedModding/UnrealEngine3
//!
//! Names and bit values come from the UE3 build-10897 headers
//! (`UnObjBas.h`, `UnStack.h`, `UnClass.h`, `UnType.h`, `UnFile.h`,
//! `UnBulkData.h`, `UnObjVer.h`, the engine build this family of games
//! ships).
//!
//! [`RF_*`]: object_flags
//! [`CPF_*`]: property_flags
//! [`FUNC_*`]: function_flags
//! [`STRUCT_*`]: struct_flags
//! [`LOAD_*`]: load_flags
//! [`BULKDATA_*`]: bulkdata_flags
//! [`LOCK_*`]: lock_flags
//! [`LOCKSTATUS_*`]: lock_flags
//! [`COMPRESS_*`]: compression_flags
//! [`CHANGE_TYPE_*`]: property_change_type
//!

pub mod object_flags {
    /// No flags set.
    pub const RF_NO_FLAGS: u64 = 0;

    /// `RF_ClassDefaultObject`: the object is its class's default object.
    /// Handy when scanning [`crate::objects::objects`] for real instances.
    pub const RF_CLASS_DEFAULT_OBJECT: u64 = 0x0000_0000_0000_0200;

    /// `RF_ArchetypeObject`: the object is a template for other objects,
    /// treat like a class default object (`UnObjBas.h`).
    pub const RF_ARCHETYPE_OBJECT: u64 = 0x0000_0000_0000_0400;

    /// `RF_RootSet`: never garbage collected, even when unreferenced. One
    /// of the two bits that make a constructed object outlive a GC pass.
    pub const RF_ROOT_SET: u64 = 0x0000_0000_0000_4000;

    /// `RF_BeginDestroyed`: `BeginDestroy` has run. Together with
    /// [`RF_FINISH_DESTROYED`] and [`RF_PENDING_KILL`] this triages dead
    /// objects before dereferencing them.
    pub const RF_BEGIN_DESTROYED: u64 = 0x0000_0000_0000_8000;

    /// `RF_FinishDestroyed`: `FinishDestroy` has run (see
    /// [`RF_BEGIN_DESTROYED`]).
    pub const RF_FINISH_DESTROYED: u64 = 0x0000_0000_0001_0000;

    /// `RF_Transactional`: participates in editor undo/redo. Mostly
    /// meaningful for `PostEditChangeProperty` style flows.
    pub const RF_TRANSACTIONAL: u64 = 0x0000_0001_0000_0000;

    /// `RF_Unreachable`: the GC found the object unreachable. A stronger
    /// "this is going away" signal than [`RF_PENDING_KILL`] during a
    /// collection.
    pub const RF_UNREACHABLE: u64 = 0x0000_0002_0000_0000;

    /// `RF_Public`: visible outside its package. The standard creation
    /// flag for objects other code is expected to find by path; combine
    /// with [`RF_STANDALONE`] for objects nothing else references.
    pub const RF_PUBLIC: u64 = 0x0000_0004_0000_0000;

    /// `RF_DisregardForGC`: the GC ignores the object entirely (it and its
    /// references are always loaded). The strongest GC survival bit.
    pub const RF_DISREGARD_FOR_GC: u64 = 0x0000_0080_0000_0000;

    /// `RF_Transient`: never saved to a package. The right flag for
    /// throwaway working objects created at runtime.
    pub const RF_TRANSIENT: u64 = 0x0000_4000_0000_0000;

    /// `RF_Standalone`: keep the object around even if unreferenced
    /// (`UnObjBas.h`). Pairs with [`RF_PUBLIC`] as the usual
    /// `construct_object` flag set.
    pub const RF_STANDALONE: u64 = 0x0008_0000_0000_0000;

    /// `RF_Native`: native class (meaningful on `UClass` objects).
    pub const RF_NATIVE: u64 = 0x0400_0000_0000_0000;

    /// `RF_PendingKill`: pending destruction. Invalid for gameplay, so
    /// skip objects with this bit in every scan.
    pub const RF_PENDING_KILL: u64 = 0x2000_0000_0000_0000;

    /// Every bit.
    pub const RF_ALL_FLAGS: u64 = u64::MAX;
}


pub mod property_flags {
    /// `CPF_Edit`: user-settable in the editor. The "this is a normal
    /// exposed variable" bit.
    pub const CPF_EDIT: u64 = 0x0000_0000_0000_0001;

    /// `CPF_Const`: never written after construction. A hard "do not
    /// `set_property` this" guard.
    pub const CPF_CONST: u64 = 0x0000_0000_0000_0002;

    /// `CPF_ExportObject`: object property whose value is exported with
    /// the actor (subobject ownership, pairs with [`CPF_EDIT_INLINE`]).
    pub const CPF_EXPORT_OBJECT: u64 = 0x0000_0000_0000_0008;

    /// `CPF_OptionalParm`: optional parameter (only with [`CPF_PARM`]).
    pub const CPF_OPTIONAL_PARM: u64 = 0x0000_0000_0000_0010;

    /// `CPF_Net`: relevant to network replication.
    pub const CPF_NET: u64 = 0x0000_0000_0000_0020;

    /// `CPF_Parm`: a function parameter. Marks properties of a `UFunction`
    /// as its parameter block, in declaration order.
    pub const CPF_PARM: u64 = 0x0000_0000_0000_0080;

    /// `CPF_OutParm`: copied out after the call (pass by reference).
    pub const CPF_OUT_PARM: u64 = 0x0000_0000_0000_0100;

    /// `CPF_SkipParm`: short-circuitable evaluation parameter.
    pub const CPF_SKIP_PARM: u64 = 0x0000_0000_0000_0200;

    /// `CPF_ReturnParm`: the function's return value. At most one per
    /// function
    pub const CPF_RETURN_PARM: u64 = 0x0000_0000_0000_0400;

    /// `CPF_CoerceParm`: coerce the argument into this parameter type.
    pub const CPF_COERCE_PARM: u64 = 0x0000_0000_0000_0800;

    /// `CPF_Native`: C++ serializes this property, UnrealScript does not.
    pub const CPF_NATIVE: u64 = 0x0000_0000_0000_1000;

    /// `CPF_Transient`: not saved, zero-filled at load. Expect garbage (or
    /// zero) values on disk round trips.
    pub const CPF_TRANSIENT: u64 = 0x0000_0000_0000_2000;

    /// `CPF_Config`: value comes from the ini/profile, not the package.
    pub const CPF_CONFIG: u64 = 0x0000_0000_0000_4000;

    /// `CPF_EditConst`: visible but uneditable in the editor. Another soft
    /// "engine-managed, do not stomp" signal (like [`CPF_CONST`] but the
    /// value does change at runtime).
    pub const CPF_EDIT_CONST: u64 = 0x0000_0000_0002_0000;

    /// `CPF_DuplicateTransient`: reset to the default on any duplication.
    pub const CPF_DUPLICATE_TRANSIENT: u64 = 0x0000_0000_0020_0000;

    /// `CPF_NeedCtorLink`: the value needs constructor/destructor work
    /// (`FString`, `TArray`, maps). Overwriting such a value raw leaks or
    /// double-frees engine memory, so it gates
    /// [`crate::objects::set_property`]'s string handling.
    pub const CPF_NEED_CTOR_LINK: u64 = 0x0000_0000_0040_0000;

    /// `CPF_EditInline`: object reference edited inline (instanced
    /// subobject semantics).
    pub const CPF_EDIT_INLINE: u64 = 0x0000_0000_0400_0000;

    /// `CPF_EditInlineUse`: [`CPF_EDIT_INLINE`] with a Use button.
    pub const CPF_EDIT_INLINE_USE: u64 = 0x0000_0000_1000_0000;

    /// `CPF_Deprecated`: still read from archives, never saved. Skip in
    /// new code.
    pub const CPF_DEPRECATED: u64 = 0x0000_0000_2000_0000;

    /// `CPF_PrivateWrite`: const outside the declaring class.
    pub const CPF_PRIVATE_WRITE: u64 = 0x0000_0000_4000_0000;

    /// `CPF_ProtectedWrite`: const outside the declaring class and its
    /// subclasses.
    pub const CPF_PROTECTED_WRITE: u64 = 0x0000_0000_8000_0000;

    /// `CPF_RepNotify`: changing the property triggers the `RepNotify`
    /// function. First bit above the SDK's 32-bit `PropertyFlags` view.
    pub const CPF_REP_NOTIFY: u64 = 0x0000_0001_0000_0000;

    /// `CPF_NonTransactional`: ignored by editor transactions.
    pub const CPF_NON_TRANSACTIONAL: u64 = 0x0000_0004_0000_0000;

    /// `CPF_EditorOnly`: only loaded in the editor. Do not expect a value
    /// in game.
    pub const CPF_EDITOR_ONLY: u64 = 0x0000_0008_0000_0000;

    /// `CPF_ArchetypeProperty`: ignored by archives with
    /// `ArIgnoreArchetypeRef` set.
    pub const CPF_ARCHETYPE_PROPERTY: u64 = 0x0000_0100_0000_0000;

    /// Engine combo `CPF_ParmFlags` (`UnObjBas.h`): every parameter
    /// bit at once. Mask a `UFunction`'s properties with this to find its
    /// parameter block.
    pub const CPF_PARM_FLAGS: u64 = CPF_OPTIONAL_PARM
        | CPF_PARM
        | CPF_OUT_PARM
        | CPF_SKIP_PARM
        | CPF_RETURN_PARM
        | CPF_COERCE_PARM;
}


pub mod function_flags {
    /// `FUNC_Final`: prebindable, non-overridable.
    pub const FUNC_FINAL: u32 = 0x0000_0001;

    /// `FUNC_Latent`: latent state function (may yield mid-execution).
    pub const FUNC_LATENT: u32 = 0x0000_0008;

    /// `FUNC_Singular`: cannot be reentered.
    pub const FUNC_SINGULAR: u32 = 0x0000_0020;

    /// `FUNC_Net`: network replicated.
    pub const FUNC_NET: u32 = 0x0000_0040;

    /// `FUNC_NetReliable`: sent reliably.
    pub const FUNC_NET_RELIABLE: u32 = 0x0000_0080;

    /// `FUNC_Simulated`: runs on the client side too.
    pub const FUNC_SIMULATED: u32 = 0x0000_0100;

    /// `FUNC_Exec`: callable from the console/command line. The bit
    /// [`crate::commands`] registrations line up with.
    pub const FUNC_EXEC: u32 = 0x0000_0200;

    /// `FUNC_Native`: implemented in C++.
    pub const FUNC_NATIVE: u32 = 0x0000_0400;

    /// `FUNC_Event`: an event (callable from both sides).
    pub const FUNC_EVENT: u32 = 0x0000_0800;

    /// `FUNC_Static`: no `self`.
    pub const FUNC_STATIC: u32 = 0x0000_2000;

    /// `FUNC_HasOptionalParms`: declares optional parameters.
    pub const FUNC_HAS_OPTIONAL_PARMS: u32 = 0x0000_4000;

    /// `FUNC_Const`: does not modify the object.
    pub const FUNC_CONST: u32 = 0x0000_8000;

    /// `FUNC_Public`: accessible in all classes.
    pub const FUNC_PUBLIC: u32 = 0x0002_0000;

    /// `FUNC_Private`: accessible only in the declaring class.
    pub const FUNC_PRIVATE: u32 = 0x0004_0000;

    /// `FUNC_Protected`: accessible in the declaring class and subclasses.
    pub const FUNC_PROTECTED: u32 = 0x0008_0000;

    /// `FUNC_Delegate`: the "function" is a delegate declaration. Skip it
    /// when enumerating callable functions.
    pub const FUNC_DELEGATE: u32 = 0x0010_0000;

    /// `FUNC_NetServer`: runs on servers.
    pub const FUNC_NET_SERVER: u32 = 0x0020_0000;

    /// `FUNC_HasOutParms`: has out (pass by reference) parameters. After
    /// the call, copy those back from the param block.
    pub const FUNC_HAS_OUT_PARMS: u32 = 0x0040_0000;

    /// `FUNC_NetClient`: runs on clients.
    pub const FUNC_NET_CLIENT: u32 = 0x0100_0000;

    /// Every bit (`FUNC_AllFlags`).
    pub const FUNC_ALL_FLAGS: u32 = 0xFFFF_FFFF;
}


pub mod struct_flags {
    /// `STRUCT_Native`: plain-old-data semantics from C++.
    pub const STRUCT_NATIVE: u32 = 0x0000_0001;

    /// `STRUCT_Export`: exported to C++ headers.
    pub const STRUCT_EXPORT: u32 = 0x0000_0002;

    /// `STRUCT_HasComponents`: contains component references.
    pub const STRUCT_HAS_COMPONENTS: u32 = 0x0000_0004;

    /// `STRUCT_Transient`: never serialized.
    pub const STRUCT_TRANSIENT: u32 = 0x0000_0008;

    /// `STRUCT_Atomic`: copy whole, do not serialize member by member
    /// (`UnClass.h` is the engine's own atomic check).
    pub const STRUCT_ATOMIC: u32 = 0x0000_0010;

    /// `STRUCT_Immutable`: no assignment at all. Read-only value.
    pub const STRUCT_IMMUTABLE: u32 = 0x0000_0020;

    /// `STRUCT_StrictConfig`: config parsing stays in this struct.
    pub const STRUCT_STRICT_CONFIG: u32 = 0x0000_0040;

    /// `STRUCT_ImmutableWhenCooked`: [`STRUCT_IMMUTABLE`] once cooked
    /// (which the shipped games are).
    pub const STRUCT_IMMUTABLE_WHEN_COOKED: u32 = 0x0000_0080;

    /// `STRUCT_AtomicWhenCooked`: [`STRUCT_ATOMIC`] once cooked.
    pub const STRUCT_ATOMIC_WHEN_COOKED: u32 = 0x0000_0100;
}

/// `ELoadFlags` (`UnObjBas.h`), `DWORD` wide. The `flags` argument
/// of [`crate::objects::load_package`] (the engine's
/// `UObject::LoadPackage(UPackage*, const TCHAR*, DWORD LoadFlags)`,
/// `UnObjBas.h`). `0` ([`load_flags::LOAD_NONE`]) is the usual choice.
pub mod load_flags {
    /// `LOAD_None`: plain load.
    pub const LOAD_NONE: u32 = 0x0000_0000;

    /// `LOAD_SeekFree`: use the seek-free loading path (cooked builds load
    /// this way by default).
    pub const LOAD_SEEK_FREE: u32 = 0x0000_0001;

    /// `LOAD_NoWarn`: no warning when the load fails (the caller checks
    /// for `null`).
    pub const LOAD_NO_WARN: u32 = 0x0000_0002;

    /// `LOAD_Throw`: throw exceptions on failure. Avoid: this crate's
    /// discipline is no exceptions across the FFI boundary.
    pub const LOAD_THROW: u32 = 0x0000_0008;

    /// `LOAD_Verify`: only verify existence, do not actually load.
    pub const LOAD_VERIFY: u32 = 0x0000_0010;

    /// `LOAD_AllowDll`: allow plain DLLs.
    pub const LOAD_ALLOW_DLL: u32 = 0x0000_0020;

    /// `LOAD_DisallowFiles`: never touch the file system.
    pub const LOAD_DISALLOW_FILES: u32 = 0x0000_0040;

    /// `LOAD_NoVerify`: skip import verification.
    pub const LOAD_NO_VERIFY: u32 = 0x0000_0080;

    /// `LOAD_Quiet`: no log warnings at all.
    pub const LOAD_QUIET: u32 = 0x0000_2000;

    /// `LOAD_FindIfFail`: fall back to `FindObject` when no linker can be
    /// obtained (e.g. the package is already loaded).
    pub const LOAD_FIND_IF_FAIL: u32 = 0x0000_4000;

    /// `LOAD_MemoryReader`: read the whole file into memory first.
    pub const LOAD_MEMORY_READER: u32 = 0x0000_8000;
}

/// `EBulkDataFlags` (`UnBulkData.h`), `DWORD` wide. The
/// `BulkDataFlags` word of an `FUntypedBulkData` (read with
/// `textures::bulkdata::MipBlock::bulk_flags`). The three
/// `BULKDATA_SERIALIZE_COMPRESSED_*` bits say which codec
/// `textures::bulkdata::disk::compression_codec` maps to
/// [`compression_flags`] ids.
pub mod bulkdata_flags {
    /// `BULKDATA_None`.
    pub const BULKDATA_NONE: u32 = 0;

    /// `BULKDATA_StoreInSeparateFile`: payload lives in a separate file
    /// (the `*.tfc` case).
    pub const BULKDATA_STORE_IN_SEPARATE_FILE: u32 = 1 << 0;

    /// `BULKDATA_SerializeCompressedZLIB`: payload compressed with zlib
    /// ([`crate::flags::compression_flags::COMPRESS_ZLIB`]).
    pub const BULKDATA_SERIALIZE_COMPRESSED_ZLIB: u32 = 1 << 1;

    /// `BULKDATA_ForceSingleElementSerialization`.
    pub const BULKDATA_FORCE_SINGLE_ELEMENT_SERIALIZATION: u32 = 1 << 2;

    /// `BULKDATA_SingleUse`: free the payload after it has been read once.
    /// `FUntypedBulkData::Unlock` frees the payload and NULLs `BulkData`
    /// when this bit is set (UnBulkData.cpp), which is the
    /// "white texture" trap for written mip blocks. The `Object.uc`
    /// mirror calls the same bit `ForceSingleElementPayload`.
    pub const BULKDATA_SINGLE_USE: u32 = 1 << 3;

    /// `BULKDATA_SerializeCompressedLZO`: payload compressed with LZO1X
    /// ([`crate::flags::compression_flags::COMPRESS_LZO`]). The codec observed on every
    /// cooked TFC payload.
    pub const BULKDATA_SERIALIZE_COMPRESSED_LZO: u32 = 1 << 4;

    /// `BULKDATA_Unused`: payload not used; do not load.
    pub const BULKDATA_UNUSED: u32 = 1 << 5;

    /// `BULKDATA_StoreOnlyPayload`.
    pub const BULKDATA_STORE_ONLY_PAYLOAD: u32 = 1 << 6;

    /// `BULKDATA_SerializeCompressedLZX`: payload compressed with LZX
    /// ([`crate::flags::compression_flags::COMPRESS_LZX`]). No LZX in the PC build's
    /// `appDecompress`, it answers 0 for this codec.
    pub const BULKDATA_SERIALIZE_COMPRESSED_LZX: u32 = 1 << 7;

    /// `BULKDATA_SerializeCompressed`: the three codec bits together. The
    /// `0x92` mask `textures::bulkdata::disk` branches on; any of the three
    /// set means the on-disk payload is a `SerializeCompressed` blob.
    pub const BULKDATA_SERIALIZE_COMPRESSED: u32 = BULKDATA_SERIALIZE_COMPRESSED_ZLIB
        | BULKDATA_SERIALIZE_COMPRESSED_LZO
        | BULKDATA_SERIALIZE_COMPRESSED_LZX;
}


pub mod lock_flags {
    /// `EBulkDataLockFlags::LOCK_READ_ONLY` (`UnBulkData.h`): read-only
    /// lock. No archive detach; `Unlock` frees an archive-backed payload
    /// again.
    pub const LOCK_READ_ONLY: u32 = 1;

    /// `EBulkDataLockFlags::LOCK_READ_WRITE` (`UnBulkData.h`): allows
    /// `Realloc`, detaches the block from its archive.
    pub const LOCK_READ_WRITE: u32 = 2;

    /// `EBulkDataLockStatus::LOCKSTATUS_Unlocked`.
    pub const LOCKSTATUS_UNLOCKED: u32 = 0;

    /// `EBulkDataLockStatus::LOCKSTATUS_ReadOnlyLock`.
    pub const LOCKSTATUS_READ_ONLY_LOCK: u32 = 1;

    /// `EBulkDataLockStatus::LOCKSTATUS_ReadWriteLock`.
    pub const LOCKSTATUS_READ_WRITE_LOCK: u32 = 2;
}


pub mod compression_flags {
    /// `COMPRESS_None`: stored uncompressed.
    pub const COMPRESS_NONE: u32 = 0x00;

    /// `COMPRESS_ZLIB`: zlib streams, the `appDecompress` codec 1.
    pub const COMPRESS_ZLIB: u32 = 0x01;

    /// `COMPRESS_LZO`: LZO1X streams, the `appDecompress` codec 2. What
    /// cooked TFC payloads use.
    pub const COMPRESS_LZO: u32 = 0x02;

    /// `COMPRESS_LZX`: LZX streams, the `appDecompress` codec 4 (the PC
    /// build's `appDecompress` answers 0 for it).
    pub const COMPRESS_LZX: u32 = 0x04;

    /// `COMPRESS_BiasMemory`: pick the codec variant that decodes into
    /// less memory.
    pub const COMPRESS_BIAS_MEMORY: u32 = 0x10;

    /// `COMPRESS_BiasSpeed`: pick the faster codec variant.
    pub const COMPRESS_BIAS_SPEED: u32 = 0x20;

    /// `COMPRESS_ForcePPUDecompressZLib`: console zlib path.
    pub const COMPRESS_FORCE_PPU_DECOMPRESS_ZLIB: u32 = 0x80;
}

/// The little-endian `PACKAGE_FILE_TAG` (`UnObjVer.h`): the magic at the
/// head of every Unreal package and of every `SerializeCompressed` blob
/// (`textures::bulkdata::disk::BLOB_MAGIC` is this value). `_SWAPPED` is the
/// big-endian spelling (`UnObjVer.h`), seen on console packages.
pub const PACKAGE_FILE_TAG: u32 = 0x9E2A_83C1;

/// `PACKAGE_FILE_TAG_SWAPPED` (`UnObjVer.h`): byte-swapped packages.
pub const PACKAGE_FILE_TAG_SWAPPED: u32 = 0xC183_2A9E;


pub mod property_change_type {
    /// `Unspecified` (`1 << 0`): the default, and what a plain property
    /// poke should send.
    pub const CHANGE_TYPE_UNSPECIFIED: u32 = 1 << 0;

    /// `ArrayAdd` (`1 << 1`): an array element was added.
    pub const CHANGE_TYPE_ARRAY_ADD: u32 = 1 << 1;

    /// `ValueSet` (`1 << 2`): a value was assigned.
    pub const CHANGE_TYPE_VALUE_SET: u32 = 1 << 2;

    /// `Duplicate` (`1 << 3`): a value was duplicated.
    pub const CHANGE_TYPE_DUPLICATE: u32 = 1 << 3;

    /// `Undo` (`1 << 4`).
    pub const CHANGE_TYPE_UNDO: u32 = 1 << 4;

    /// `Redo` (`1 << 5`).
    pub const CHANGE_TYPE_REDO: u32 = 1 << 5;
}
