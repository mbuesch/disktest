disktest - Hard Disk (HDD) and Solid State Disk (SSD) tester
============================================================

`https://bues.ch/h/disktest <https://bues.ch/h/disktest>`_

Disktest is a tool to check Hard Disks, Solid State Disks, USB sticks, SD cards or similar storage media for errors.

It does so by writing a pseudo random sequence to the device and then reading it back and verifying it to the expected pseudo random sequence.

This tool can be used to:

* Check disks for hardware errors (e.g. platter errors, Flash errors, etc...).
* Overwrite storage media with a cryptographically strong pseudo random stream. This can either be used to delete existing data on the disk, or to prepare the disk for encryption.
* Test for tampered media that pretend to have more storage area than they physically actually have. Sometimes such media are sold by fraudulent sellers for cheap prices.
* ... probably lots of other tasks.


Dependencies
============

* `Rust (edition 2018) <https://www.rust-lang.org/>`_ or later.


License
=======

Copyright (c) 2020 Michael Buesch <m@bues.ch>

Licensed under the terms of the GNU General Public License version 2, or (at your option) any later version.
