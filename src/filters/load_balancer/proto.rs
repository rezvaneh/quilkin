/*
 * Copyright 2021 Google LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *       http://www.apache.org/licenses/LICENSE-2.0
 *
 *  Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 */

/// Protobuf config for this filter.
pub(super) mod quilkin {
    pub mod extensions {
        pub mod filters {
            pub mod load_balancer {
                pub mod v1alpha1 {
                    #![doc(hidden)]
                    tonic::include_proto!("quilkin.filters.load_balancer.v1alpha1");
                }
            }
        }
    }
}
