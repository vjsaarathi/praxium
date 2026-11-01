# Praxium Core

Praxium Core provides reusable storage and interaction capabilities for schema-governed JSON objects. It does not interpret object fields or define application workflows.

## Language

**Collection**:
A named group of JSON objects governed by a Collection Schema.

**Collection Schema**:
A user-managed JSON Schema that defines the allowed shape of objects in a Collection.

**Stored Object**:
A JSON object held by Core with identity and revision metadata managed separately from its value.
_Avoid_: Task, Record

**Schema Version**:
A retained revision of a Collection Schema.

**Schema Migration**:
A client-supplied transformation that changes stored objects to fit a new Collection Schema.

**Collection Index**:
A client-declared data lookup structure for values within a Collection.

**Core Initialization Manifest**:
Client-supplied defaults used to initialize collections and their definitions in a Core database.

**Client**:
An application that uses the Core API and assigns application-specific meaning to Stored Objects.

**Client Plugin**:
An extension hosted by a Client that contributes application behavior or default Collection definitions through the Core API.
_Avoid_: Core Plugin

**Object Revision**:
The Core-assigned version of a Stored Object, used to identify its state and detect stale writes.

**Change Entry**:
A record of a successful change to a Stored Object.

**Change History**:
The ordered Change Entries for a Stored Object.

**Soft Delete**:
A reversible operation that hides a Stored Object while retaining it.

**Purge**:
An operation that permanently removes a Stored Object and its Change History.

**Revision Restore**:
An operation that returns a Stored Object to content from an earlier Object Revision.

**Client Undo/Redo**:
A Client-managed action sequence built using Change History and Revision Restore.
