mod single {
    use crate::models::common::{Demand, Load, SingleDimLoad};

    fn from_value(load: i32) -> SingleDimLoad {
        SingleDimLoad::new(load)
    }

    #[test]
    fn can_sum_dimens() {
        assert_eq!(from_value(1) + from_value(2), from_value(3));
        assert_eq!(from_value(1) + from_value(0), from_value(1));

        assert_eq!(SingleDimLoad::default() + from_value(0), SingleDimLoad::default());
        assert_eq!(SingleDimLoad::default() + SingleDimLoad::default(), SingleDimLoad::default());
    }

    #[test]
    fn can_sub_dimens() {
        assert_eq!(from_value(3) - from_value(2), from_value(1));
        assert_eq!(from_value(1) - from_value(0), from_value(1));

        assert_eq!(SingleDimLoad::default() - from_value(0), SingleDimLoad::default());
        assert_eq!(SingleDimLoad::default() - SingleDimLoad::default(), SingleDimLoad::default());
    }

    #[test]
    fn can_compare_dimens() {
        assert!(from_value(2) > from_value(1));
        assert!(from_value(1) < from_value(3));
        assert!(from_value(5) >= from_value(2));

        assert!(from_value(2) < from_value(5));

        assert_eq!(from_value(0), SingleDimLoad::default());
        assert_eq!(SingleDimLoad::default(), SingleDimLoad::default());
    }

    #[test]
    fn can_use_specific_functions() {
        assert!(from_value(1).is_not_empty());
        assert!(!from_value(0).is_not_empty());

        assert_eq!(from_value(10).max_load(from_value(5)), from_value(10));
        assert_eq!(from_value(5).max_load(from_value(10)), from_value(10));

        assert!(from_value(10).can_fit(&from_value(5)));
        assert!(!from_value(5).can_fit(&from_value(10)));
    }

    #[test]
    fn can_use_pudo_simple_ctors() {
        assert_pudo(Demand::pickup(1), (1, 0, 0, 0));
        assert_pudo(Demand::delivery(1), (0, 0, 1, 0));
    }

    #[test]
    fn can_use_pudo_demand_ctors() {
        assert_pudo(Demand::pudo_pickup(1), (0, 1, 0, 0));
        assert_pudo(Demand::pudo_delivery(1), (0, 0, 0, 1));
    }

    fn assert_pudo(pudo: Demand<SingleDimLoad>, expected: (i32, i32, i32, i32)) {
        assert_eq!(pudo.pickup.0, SingleDimLoad::new(expected.0));
        assert_eq!(pudo.pickup.1, SingleDimLoad::new(expected.1));
        assert_eq!(pudo.delivery.0, SingleDimLoad::new(expected.2));
        assert_eq!(pudo.delivery.1, SingleDimLoad::new(expected.3));
    }
}

mod multi {
    use crate::models::common::{Demand, DemandType, Load, MultiDimLoad};
    use std::cmp::Ordering;

    fn from_vec(load: Vec<i32>) -> MultiDimLoad {
        MultiDimLoad::new(load)
    }

    #[test]
    fn can_use_sixteen_dimensions_without_losing_high_dimensions() {
        let values = (1..=16).collect::<Vec<_>>();
        let load = from_vec(values.clone());
        let one = from_vec(vec![1; 16]);
        assert_eq!(load.as_vec(), values);
        assert_eq!((load + one).as_vec(), (2..=17).collect::<Vec<_>>());
        assert_eq!((load - one).as_vec(), (0..=15).collect::<Vec<_>>());

        let mut larger = values.clone();
        larger[15] += 1;
        let larger = from_vec(larger);
        assert!(!load.can_fit(&larger));
        assert!(larger.can_fit(&load));
        // Preserve the existing ordering semantics: equal early counters and
        // a different final counter are incomparable, not equal.
        assert_eq!(load.partial_cmp(&larger), None);
        assert_eq!(load.partial_cmp(&(load + one)), Some(Ordering::Less));
        assert_eq!(load.max_load(larger), larger);
        assert!(from_vec(vec![0; 15]).can_fit(&MultiDimLoad::default()));
        assert!(!from_vec(vec![0; 15]).can_fit(&from_vec([vec![0; 15], vec![1]].concat())));
    }

    #[test]
    #[should_panic]
    fn rejects_more_than_sixteen_dimensions() {
        from_vec(vec![1; 17]);
    }

    #[test]
    fn can_sum_dimens() {
        assert_eq!((from_vec(vec![1, 0, 2]) + from_vec(vec![3, 1, 0])), from_vec(vec![4, 1, 2]));
        assert_eq!((from_vec(vec![1, 0, 0]) + from_vec(vec![0, 0, 0])), from_vec(vec![1, 0, 0]));

        assert_eq!((from_vec(vec![1]) + from_vec(vec![0, 0, 2])), from_vec(vec![1, 0, 2]));
        assert_eq!((from_vec(vec![0, 0, 2]) + from_vec(vec![1])), from_vec(vec![1, 0, 2]));

        assert_ne!((from_vec(vec![1, 0, 2]) + from_vec(vec![3, 1, 0])), from_vec(vec![3, 1, 2]));
    }

    #[test]
    fn can_sub_dimens() {
        assert_eq!((from_vec(vec![3, 0, 2]) - from_vec(vec![1, 1, 4])), from_vec(vec![2, -1, -2]));
        assert_eq!((from_vec(vec![3, 0, 2]) - from_vec(vec![0, 0, 0])), from_vec(vec![3, 0, 2]));

        assert_eq!((from_vec(vec![1]) - from_vec(vec![0, 0, 2])), from_vec(vec![1, 0, -2]));
        assert_eq!((from_vec(vec![0, 0, 2]) - from_vec(vec![1])), from_vec(vec![-1, 0, 2]));

        assert_ne!((from_vec(vec![1, 0, 2]) - from_vec(vec![3, 1, 0])), from_vec(vec![3, 1, 2]));
    }

    #[test]
    fn can_compare_dimens() {
        assert_eq!(from_vec(vec![3, 0, 2]).partial_cmp(&from_vec(vec![1, 1, 4])), None);
        assert_eq!(from_vec(vec![1, 0, 2]).partial_cmp(&from_vec(vec![3, 3, 3])), Some(Ordering::Less));
        assert_eq!(from_vec(vec![3, 3, 3]).partial_cmp(&from_vec(vec![1, 0, 2])), Some(Ordering::Greater));

        assert_eq!(from_vec(vec![3]).partial_cmp(&from_vec(vec![1, 1, 4])), None);

        assert_eq!(from_vec(vec![0, 0, 2]).partial_cmp(&from_vec(vec![0, 0, 2])), Some(Ordering::Equal));
        assert_eq!(from_vec(vec![1, 0, 0]).partial_cmp(&from_vec(vec![1])), Some(Ordering::Equal));
        assert_eq!(from_vec(vec![0, 0, 0]).partial_cmp(&MultiDimLoad::default()), Some(Ordering::Equal));
    }

    #[test]
    fn can_use_specific_functions() {
        assert!(from_vec(vec![1, 0]).is_not_empty());
        assert!(!from_vec(vec![0, 0]).is_not_empty());
        assert!(!MultiDimLoad::default().is_not_empty());

        assert_eq!(from_vec(vec![0, 1]).max_load(from_vec(vec![1, 0])), from_vec(vec![1, 1]));
        assert_eq!(from_vec(vec![3, 0, 2]).max_load(from_vec(vec![1, 1, 4])), from_vec(vec![3, 1, 4]));
        assert_eq!(from_vec(vec![3]).max_load(from_vec(vec![1, 1, 4])), from_vec(vec![3, 1, 4]));

        assert!(!from_vec(vec![1, 0]).can_fit(&from_vec(vec![0, 1])));
        assert!(!from_vec(vec![3, 0, 2]).can_fit(&from_vec(vec![1, 1, 4])));
        assert!(!from_vec(vec![3]).can_fit(&from_vec(vec![1, 1])));
    }

    #[test]
    fn can_get_static_delivery_demand_type() {
        let demand = Demand::<MultiDimLoad> {
            pickup: Default::default(),
            delivery: (from_vec(vec![1, 2]), MultiDimLoad::default()),
        };

        assert!(matches!(demand.get_type(), DemandType::Delivery));
        assert!(!demand.has_dynamic());
    }

    #[test]
    fn can_detect_dynamic_demand() {
        let demand = Demand::<MultiDimLoad> {
            pickup: (MultiDimLoad::default(), from_vec(vec![1, 2])),
            delivery: Default::default(),
        };

        assert!(demand.has_dynamic());
    }
}

mod configurable {
    use crate::models::common::{ConfigurableLoad, Load};

    #[test]
    fn can_enforce_sixteenth_dimension_across_capacity_layouts() {
        let mut first = vec![2; 16];
        first[15] = 0;
        let mut second = vec![1; 16];
        second[15] = 2;
        let capacity = ConfigurableLoad::new(vec![first.clone(), second.clone()]);
        assert_eq!(capacity.configurations_as_vec(), vec![first, second.clone()]);
        assert!(capacity.can_fit(&ConfigurableLoad::from_load(second.clone())));
        second[15] = 3;
        assert!(!capacity.can_fit(&ConfigurableLoad::from_load(second)));
    }

    #[test]
    #[should_panic]
    fn rejects_layouts_with_more_than_sixteen_dimensions() {
        ConfigurableLoad::new(vec![vec![1; 17]]);
    }

    #[test]
    #[should_panic]
    fn rejects_demand_with_more_than_sixteen_dimensions() {
        ConfigurableLoad::from_load(vec![1; 17]);
    }

    #[test]
    fn default_load_is_empty() {
        assert!(!ConfigurableLoad::default().is_not_empty());
        assert!(!ConfigurableLoad::from_load(vec![0, 0]).is_not_empty());
        assert!(ConfigurableLoad::from_load(vec![0, 1]).is_not_empty());
    }
}
