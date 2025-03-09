use std::{collections::HashSet, sync::Arc};

use srad_types::{metric::MetricValidToken, payload::{self, DataType, MetaData}, property_set::PropertySet, MetricId, MetricValue};


pub struct MetricToken {
   name: Arc<String>,
   alias: Option<u64>,
   datatype: DataType,
   valid: MetricValidToken
}

impl MetricToken {

    fn new(name: Arc<String>, alias: Option<u64>, datatype:DataType, valid: MetricValidToken) -> Self {
        Self {
            name, 
            alias,
            datatype,
            valid
        }
    }

}

pub struct MetricDetails {
    value: Option<MetricValue>,
    properties: Option<PropertySet>,
    metadata: Option<MetaData>,
    timestamp: u64,
    is_historical: bool,
    is_transient: bool
}

macro_rules! metric_details_try_from_payload_metric {
    ($metric:expr) => {{
        let timestamp = $metric.timestamp.ok_or(())?;
        
        let value = if let Some(value) = $metric.value {
            Some(value.into())
        } else if let Some(is_null) = $metric.is_null {
            if is_null { None } else { return Err(()) }
        } else { 
            return Err(()) 
        };

        let properties = match $metric.properties {
            Some(ps) => Some(ps.try_into()?),
            None => None,
        };

        let metadata = $metric.metadata;
        let is_historical = $metric.is_historical.unwrap_or(false);
        let is_transient = $metric.is_transient.unwrap_or(false);

        Ok(MetricDetails {
            value,
            properties,
            metadata,
            timestamp,
            is_historical,
            is_transient,
        })
    }}
}

pub(crate) fn get_metric_id_and_details_from_payload_metrics(metrics: Vec<payload::Metric>) -> Result<Vec<(MetricId, MetricDetails)>, ()> {
    let mut metric_id_details = Vec::with_capacity(metrics.len());
    for x in metrics {
        let id = if let Some(alias) = x.alias {
            MetricId::Alias(alias)
        } else if let Some(name) = x.name {
            MetricId::Name(name)
        } else { return Err(()) };
        let details = metric_details_try_from_payload_metric!(x)?; 
        metric_id_details.push((id, details))
    }
    Ok(metric_id_details) 
}

pub(crate) struct MetricStore {
    metrics_valid_token: MetricValidToken
}


impl MetricStore {

    fn create_metric_tokens_from_birth_metrics(metrics: Vec<payload::Metric>) -> Result<(MetricValidToken, Vec<(MetricToken, MetricDetails)>),()> {
        //make sure metrics names and aliases are unique
        let mut metric_name_map = HashSet::with_capacity(metrics.len());
        let mut metric_alias_map = HashSet::with_capacity(metrics.len());
        let mut results= Vec::with_capacity(metrics.len());

        let valid_token = MetricValidToken::new();

        for x in metrics {

            let datatype = x.datatype.ok_or(())?.try_into()?;
            let name = Arc::new(x.name.ok_or(())?);
            if metric_name_map.insert(name.clone()) == false {
                //for now return an error, however it may be valid to provide multiple metric 
                //values for the same metric in a birth payload? seems a bit silly tho 
                return Err(())
            }

            let alias = x.alias;
            if let Some(alias) = alias {
                if metric_alias_map.insert(alias) == false {
                    return Err(())
                }
            } 


            let token = MetricToken::new(name.clone(), alias, datatype, valid_token.clone());
            let details = metric_details_try_from_payload_metric!(x)?;

            results.push((token, details));
        }

        Ok((valid_token, results)) 

    } 

    fn new(metrics: Vec<payload::Metric>) -> Result<(Self, Vec<(MetricToken, MetricDetails)>), ()> {
        let (valid_token, metrics) = Self::create_metric_tokens_from_birth_metrics(metrics)?;
        Ok((Self { metrics_valid_token: valid_token }, metrics))
    }

    fn update(&mut self, metrics: Vec<payload::Metric>) -> Result<Vec<(MetricToken, MetricDetails)>, ()> {
        let (valid_token, metrics) = Self::create_metric_tokens_from_birth_metrics(metrics)?;
        self.metrics_valid_token.invalidate();
        self.metrics_valid_token = valid_token;
        Ok(metrics)
    }

}

