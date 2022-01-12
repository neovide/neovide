#!/usr/bin/awk -f
#
# Copyright 2021 Clément Joly
# https://cj.rs/riss
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Divide the input file into these types of sections:
# * inserting: the current line is copied to the output, verbatim
# * removing: the line is not copied to the output
# To distinguish between these sections, the script interprets special
# comments. These comments are removed from the output.
BEGIN {
	# Section identifier
	removing = 0
}

/^<!--+ remove -+->$/ {
	removing = 1
	next
}

/^<!--+ end_remove -+->$|^<!--+ insert$|^end_insert -+->$/ {
	removing = 0
	next
}

! removing {
	print $0
}

