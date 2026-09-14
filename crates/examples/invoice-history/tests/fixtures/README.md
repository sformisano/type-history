# Invoice and payment fixtures

The `.msgpack.hex` files contain stored invoices and payments as space-separated
hexadecimal bytes. They were written from the MessagePack format, independently
of the project's serializer. This lets a test catch a format change even if the
new serializer and decoder still agree with each other. The payment fixtures
also include the corresponding JSON envelopes.

The envelopes use the fixed map order `stable_name`, `version`, `payload`.
Payload keys follow the field order of the stored version: V1 or V3 for invoices,
and V1 or V2 for payments. Maps, strings, and positive integers use their smallest
available MessagePack representation.
The tests decode these bytes into separately written expected values and also
require named serialization to reproduce every byte exactly.

Keep these historical bytes when changing the serializer. Add a new fixture for
a new case; replacing an old one would stop checking whether old data is still
readable.
